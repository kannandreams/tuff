use tuff_core::catalog;
use tuff_core::manifest::{McpServerConfig, McpTransport};

use crate::error::Result;

use super::render_table;

/// List the built-in MCP catalog.
///
/// `tuff mcp search` reaches the registry; this is the curated list compiled
/// into the binary, and until now nothing could enumerate it. The website's
/// catalog page had to parse the TOML asset itself for want of this command,
/// and an editor integration had no way to offer the list at all.
///
/// The fields mirror what `website/scripts/sync-mcp-catalog.mjs` derives, so
/// the page and the CLI describe an entry the same way.
pub fn cmd_mcp_catalog(json: bool) -> Result<()> {
    let entries = entries()?;

    if json {
        let rows: Vec<serde_json::Value> = entries
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "id": entry.id,
                    "version": entry.version,
                    "description": entry.description,
                    "transport": entry.transport,
                    "command": entry.command,
                    "variables": entry.variables,
                    "needs_key": !entry.variables.is_empty(),
                    "tools": entry.tools,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }

    let rows: Vec<Vec<String>> = entries
        .iter()
        .map(|entry| {
            vec![
                entry.id.clone(),
                entry.version.clone(),
                entry.transport.clone(),
                if entry.variables.is_empty() {
                    "—".to_string()
                } else {
                    entry.variables.join(", ")
                },
                truncate(&entry.description, 60),
            ]
        })
        .collect();

    println!(
        "{}",
        render_table(
            &["ID", "VERSION", "TRANSPORT", "VARIABLES", "DESCRIPTION"],
            &rows
        )
    );
    println!("install one with: tuff add mcp <ID>");
    Ok(())
}

struct Entry {
    id: String,
    version: String,
    description: String,
    transport: String,
    command: String,
    variables: Vec<String>,
    tools: Vec<String>,
}

/// Every catalog entry, resolved through the same `lookup` that `add` uses.
///
/// Resolving rather than re-reading the TOML means a malformed entry fails
/// here exactly as it would at install time, so this command can never
/// advertise a server `tuff add mcp` would refuse.
fn entries() -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for id in catalog::ids() {
        let Some(manifest) = catalog::lookup(&id)? else {
            continue;
        };
        let Some(server) = manifest.server.as_ref() else {
            continue;
        };
        entries.push(Entry {
            id: manifest.id.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            transport: match server.transport {
                McpTransport::Http => "http".to_string(),
                _ => "stdio".to_string(),
            },
            command: invocation(server),
            variables: catalog::required_env(server),
            tools: tools(server),
        });
    }
    Ok(entries)
}

/// The command a harness would actually run, or the URL for a remote server.
fn invocation(server: &McpServerConfig) -> String {
    if let McpTransport::Http = server.transport {
        return server.url.clone().unwrap_or_default();
    }
    let mut parts = Vec::new();
    if let Some(command) = server.command.as_ref() {
        parts.push(command.clone());
    }
    parts.extend(server.args.iter().cloned());
    parts.join(" ")
}

fn tools(server: &McpServerConfig) -> Vec<String> {
    server
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.tools_summary.as_ref())
        .map(|summary| {
            summary
                .split(',')
                .map(|tool| tool.trim().to_string())
                .filter(|tool| !tool.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn truncate(value: &str, max: usize) -> String {
    let single_line = value.replace(['\n', '\r'], " ");
    if single_line.chars().count() <= max {
        return single_line;
    }
    let kept: String = single_line.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}
