use std::path::Path;

use crate::error::Result;

pub fn register_tool(
    mcp_config_path: &Path,
    tool_id: &str,
    command: &str,
    args: &[String],
) -> Result<()> {
    let mut config: serde_json::Value = if mcp_config_path.exists() {
        let raw = std::fs::read_to_string(mcp_config_path)?;
        if raw.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&raw).unwrap_or(serde_json::json!({}))
        }
    } else {
        serde_json::json!({})
    };

    let servers = config
        .as_object_mut()
        .and_then(|object| {
            object
                .entry("mcpServers")
                .or_insert_with(|| serde_json::json!({}))
                .as_object_mut()
        })
        .expect("new JSON object always has a mutable object map");

    servers.insert(
        tool_id.to_string(),
        serde_json::json!({"command": command, "args": args}),
    );

    if let Some(parent) = mcp_config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        mcp_config_path,
        serde_json::to_string_pretty(&config)? + "\n",
    )?;
    Ok(())
}

pub fn remove_tool(mcp_config_path: &Path, tool_id: &str) -> Result<()> {
    if !mcp_config_path.exists() {
        return Ok(());
    }

    let raw = std::fs::read_to_string(mcp_config_path)?;
    let mut config: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::json!({}));

    if let Some(servers) = config
        .as_object_mut()
        .and_then(|object| object.get_mut("mcpServers"))
        .and_then(serde_json::Value::as_object_mut)
    {
        servers.remove(tool_id);
    }

    std::fs::write(
        mcp_config_path,
        serde_json::to_string_pretty(&config)? + "\n",
    )?;
    Ok(())
}
