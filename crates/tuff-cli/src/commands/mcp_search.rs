use crate::error::Result;
use crate::registry;

use super::{block_on_oci, render_table};

/// Search the MCP registry and show what each hit would install.
///
/// The INSTALL column is the useful part: it says whether `tuff add mcp`
/// can express this entry at all, and if not, why. Finding out after the
/// install attempt would be a worse experience than seeing it in the list.
pub fn cmd_mcp_search(query: &str, limit: usize, registry_url: &str, json: bool) -> Result<()> {
    let servers = block_on_oci(registry::search(registry_url, query, limit))?;

    if json {
        let rows: Vec<serde_json::Value> = servers
            .iter()
            .map(|server| {
                let id = registry::default_capability_id(&server.name);
                let (installable, detail) = match registry::to_manifest(server, &id) {
                    Ok(_) => (true, None),
                    Err(error) => (false, Some(error.to_string())),
                };
                serde_json::json!({
                    "name": server.name,
                    "version": server.version,
                    "description": server.description,
                    "id": id,
                    "installable": installable,
                    "detail": detail,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }

    if servers.is_empty() {
        println!("no servers in the registry match '{query}'");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = servers
        .iter()
        .map(|server| {
            let id = registry::default_capability_id(&server.name);
            let install = match registry::to_manifest(server, &id) {
                Ok(manifest) => manifest
                    .server
                    .as_ref()
                    .and_then(|config| config.command.clone())
                    .unwrap_or_else(|| "http".to_string()),
                Err(_) => "unsupported".to_string(),
            };
            vec![
                server.name.clone(),
                server.version.clone(),
                install,
                truncate(&server.description, 60),
            ]
        })
        .collect();

    println!(
        "{}",
        render_table(&["NAME", "VERSION", "INSTALL", "DESCRIPTION"], &rows)
    );
    println!("install one with: tuff add mcp <NAME>");
    Ok(())
}

fn truncate(value: &str, max: usize) -> String {
    let single_line = value.replace(['\n', '\r'], " ");
    if single_line.chars().count() <= max {
        return single_line;
    }
    let kept: String = single_line.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}
