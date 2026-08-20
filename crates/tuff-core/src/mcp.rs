use std::path::Path;

use crate::error::{Result, TuffError};

pub fn validate_config(mcp_config_path: &Path) -> Result<()> {
    read_config(mcp_config_path).map(|_| ())
}

pub fn register_tool(
    mcp_config_path: &Path,
    tool_id: &str,
    command: &str,
    args: &[String],
) -> Result<()> {
    let mut config = read_config(mcp_config_path)?;
    let config_object = config
        .as_object_mut()
        .expect("read_config returns a JSON object");
    let servers = config_object
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .expect("read_config validates the mcpServers object");

    servers.insert(
        tool_id.to_string(),
        serde_json::json!({"command": command, "args": args}),
    );

    if let Some(parent) = mcp_config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_config(mcp_config_path, &config)
}

pub fn remove_tool(mcp_config_path: &Path, tool_id: &str) -> Result<()> {
    if !mcp_config_path.exists() {
        return Ok(());
    }

    let mut config = read_config(mcp_config_path)?;
    let Some(servers) = config
        .as_object_mut()
        .and_then(|object| object.get_mut("mcpServers"))
        .and_then(serde_json::Value::as_object_mut)
    else {
        return Ok(());
    };
    if servers.remove(tool_id).is_none() {
        return Ok(());
    }

    write_config(mcp_config_path, &config)
}

fn read_config(mcp_config_path: &Path) -> Result<serde_json::Value> {
    let config = if mcp_config_path.exists() {
        let raw = std::fs::read_to_string(mcp_config_path)?;
        if raw.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&raw).map_err(|error| {
                TuffError::new(format!(
                    "invalid MCP config at {}: {error}",
                    mcp_config_path.display()
                ))
            })?
        }
    } else {
        serde_json::json!({})
    };

    let object = config.as_object().ok_or_else(|| {
        TuffError::new(format!(
            "invalid MCP config at {}: root must be a JSON object",
            mcp_config_path.display()
        ))
    })?;
    if object
        .get("mcpServers")
        .is_some_and(|servers| !servers.is_object())
    {
        return Err(TuffError::new(format!(
            "invalid MCP config at {}: field 'mcpServers' must be a JSON object",
            mcp_config_path.display()
        )));
    }

    Ok(config)
}

fn write_config(mcp_config_path: &Path, config: &serde_json::Value) -> Result<()> {
    std::fs::write(
        mcp_config_path,
        serde_json::to_string_pretty(config)? + "\n",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_rejects_malformed_json_without_changing_it() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("mcp.json");
        let original = b"{ not-json\n";
        std::fs::write(&path, original).expect("write config");

        let error = register_tool(&path, "demo", "python", &[]).expect_err("invalid config");

        assert!(error.to_string().contains("invalid MCP config"));
        assert_eq!(std::fs::read(&path).expect("read config"), original);
    }

    #[test]
    fn remove_rejects_malformed_json_without_changing_it() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("mcp.json");
        let original = b"[invalid";
        std::fs::write(&path, original).expect("write config");

        let error = remove_tool(&path, "demo").expect_err("invalid config");

        assert!(error.to_string().contains("invalid MCP config"));
        assert_eq!(std::fs::read(&path).expect("read config"), original);
    }

    #[test]
    fn register_preserves_unrelated_fields() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("mcp.json");
        std::fs::write(
            &path,
            r#"{"custom":{"enabled":true},"mcpServers":{"existing":{"command":"node"}}}"#,
        )
        .expect("write config");

        register_tool(&path, "demo", "python", &["server.py".to_string()]).expect("register tool");

        let config: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).expect("read config"))
                .expect("parse config");
        assert_eq!(config["custom"]["enabled"], true);
        assert_eq!(config["mcpServers"]["existing"]["command"], "node");
        assert_eq!(config["mcpServers"]["demo"]["command"], "python");
    }

    #[test]
    fn validation_rejects_non_object_mcp_servers() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("mcp.json");
        std::fs::write(&path, r#"{"mcpServers":[]}"#).expect("write config");

        let error = validate_config(&path).expect_err("invalid mcpServers");

        assert!(
            error
                .to_string()
                .contains("'mcpServers' must be a JSON object")
        );
    }
}
