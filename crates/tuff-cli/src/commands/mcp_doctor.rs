//! `tuff mcp doctor` (RFC-102 stage d) — spawn each installed `mcp-server`
//! capability's real server, complete the MCP handshake, and report whether
//! it's actually reachable, not just configured. See `crate::mcp_client`
//! for the protocol probe itself.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use crate::error::Result;
use crate::lockfile;
use crate::manifest::CapabilityType;
use crate::mcp_client::{self, ProbeReport};

use super::{home_dir, paint, render_table};

#[derive(Serialize)]
struct DoctorRow {
    id: String,
    transport: String,
    harnesses: Vec<String>,
    status: String,
    detail: String,
    tools: Vec<String>,
}

pub fn cmd_mcp_doctor(
    repo_root: &Path,
    agent_filter: &[String],
    global: bool,
    json: bool,
    ignore_failures: bool,
    timeout_secs: u64,
) -> Result<()> {
    let (scope_root, scope) = if global {
        (home_dir()?, crate::resolver::Scope::Global)
    } else {
        (repo_root.to_path_buf(), crate::resolver::Scope::Project)
    };
    let lf = lockfile::require_scoped_lockfile(&scope_root, scope)?;

    let mut candidates: Vec<(String, crate::manifest::McpServerConfig, Vec<String>)> = lf
        .capabilities
        .iter()
        .filter(|(_, entry)| entry.capability_type == CapabilityType::McpServer)
        .filter_map(|(id, entry)| {
            let server = entry.server.clone()?;
            let mut harnesses: Vec<String> = entry.targets.keys().cloned().collect();
            harnesses.sort();
            if !agent_filter.is_empty() && !harnesses.iter().any(|h| agent_filter.contains(h)) {
                return None;
            }
            Some((id.clone(), server, harnesses))
        })
        .collect();
    candidates.sort_by(|a, b| a.0.cmp(&b.0));

    if candidates.is_empty() {
        println!(
            "no mcp-server capabilities installed in {} scope",
            if global { "global" } else { "project" }
        );
        return Ok(());
    }

    let timeout = Duration::from_secs(timeout_secs);
    let reports: Vec<ProbeReport> = super::block_on_oci(async {
        let mut reports = Vec::with_capacity(candidates.len());
        for (_, server, _) in &candidates {
            reports.push(mcp_client::probe(server, timeout).await);
        }
        Ok(reports)
    })?;

    let rows: Vec<DoctorRow> = candidates
        .into_iter()
        .zip(reports)
        .map(|((id, server, harnesses), report)| DoctorRow {
            id,
            transport: server.transport.as_str().to_string(),
            harnesses,
            status: report.status.to_string(),
            detail: report.detail,
            tools: report.tools,
        })
        .collect();

    let all_ok = rows.iter().all(|row| row.status == "ok");

    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else {
        let table_rows: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                vec![
                    row.id.clone(),
                    row.transport.clone(),
                    row.harnesses.join(", "),
                    style_doctor_status(&row.status),
                    row.detail.clone(),
                ]
            })
            .collect();
        println!(
            "{}",
            render_table(
                &["ID", "TRANSPORT", "HARNESSES", "STATUS", "DETAIL"],
                &table_rows
            )
        );
    }

    if !all_ok && !ignore_failures {
        std::process::exit(1);
    }
    Ok(())
}

fn style_doctor_status(status: &str) -> String {
    match status {
        "ok" => format!("{} {}", paint("✓", "32"), paint("ok", "32")),
        "missing env" => format!("{} {}", paint("?", "33"), paint("missing env", "33")),
        "unsupported transport" => format!(
            "{} {}",
            paint("?", "2"),
            paint("unsupported transport", "2")
        ),
        other => format!("{} {}", paint("✗", "31"), paint(other, "31")),
    }
}
