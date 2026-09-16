use std::path::Path;

use crate::error::{Result, TuffError};
use crate::policy::{OPENCODE_SCHEMA, OrderedJson, is_opencode_config};

/// The object MCP servers live under: `mcpServers` in a harness's MCP
/// config file, `mcp` in OpenCode's `opencode.json`.
pub fn servers_key(mcp_config_path: &Path) -> &'static str {
    if is_opencode(mcp_config_path) {
        "mcp"
    } else {
        "mcpServers"
    }
}

/// Whether the file is an OpenCode config, whose other keys (`permission`)
/// are order-sensitive, so it is read and written keeping key order.
fn is_opencode(mcp_config_path: &Path) -> bool {
    mcp_config_path.to_str().is_some_and(is_opencode_config)
}

pub fn validate_config(mcp_config_path: &Path) -> Result<()> {
    read_config(mcp_config_path).map(|_| ())
}

pub fn register_tool(
    mcp_config_path: &Path,
    tool_id: &str,
    command: &str,
    args: &[String],
) -> Result<()> {
    register_server(
        mcp_config_path,
        tool_id,
        serde_json::json!({"command": command, "args": args}),
        true,
    )
}

/// Insert or replace `mcpServers.<id>` with `entry`, leaving every other
/// key in the file untouched.
///
/// With `allow_overwrite = false` an existing entry under the same id is a
/// hard error: MCP config files are shared ground that users hand-edit, and
/// for an `mcp-server` capability the JSON entry *is* the product, so
/// clobbering one Tuff never wrote would violate the never-silently-
/// overwrite invariant. Callers pass `true` only when the lockfile already
/// tracks that id for this target.
pub fn register_server(
    mcp_config_path: &Path,
    server_id: &str,
    entry: serde_json::Value,
    allow_overwrite: bool,
) -> Result<()> {
    if is_opencode(mcp_config_path) {
        return register_server_ordered(mcp_config_path, server_id, entry, allow_overwrite);
    }
    let key = servers_key(mcp_config_path);
    let mut config = read_config(mcp_config_path)?;
    let config_object = config
        .as_object_mut()
        .expect("read_config returns a JSON object");
    let servers = config_object
        .entry(key)
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .expect("read_config validates the servers object");

    if !allow_overwrite && servers.contains_key(server_id) {
        return Err(untracked_server(mcp_config_path, server_id));
    }

    servers.insert(server_id.to_string(), entry);

    if let Some(parent) = mcp_config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_config(mcp_config_path, &config)
}

fn untracked_server(mcp_config_path: &Path, server_id: &str) -> TuffError {
    TuffError::refused(format!(
        "refusing to overwrite untracked MCP server '{}' in {}",
        server_id,
        mcp_config_path.display()
    ))
    .with_hint("remove it by hand, or choose a different capability id")
}

/// `register_server` for an OpenCode config. Every other key keeps its
/// position, since OpenCode reads the order of `permission` rules as
/// precedence, and a new file starts with OpenCode's `$schema`.
fn register_server_ordered(
    mcp_config_path: &Path,
    server_id: &str,
    entry: serde_json::Value,
    allow_overwrite: bool,
) -> Result<()> {
    let mut root = read_ordered(mcp_config_path)?;
    let OrderedJson::Object(object) = &mut root else {
        return Err(corrupt(mcp_config_path, "root must be a JSON object"));
    };
    let servers = object
        .entry("mcp".to_string())
        .or_insert_with(|| OrderedJson::Object(indexmap::IndexMap::new()));
    let OrderedJson::Object(servers) = servers else {
        return Err(corrupt(
            mcp_config_path,
            "field 'mcp' must be a JSON object",
        ));
    };
    if !allow_overwrite && servers.contains_key(server_id) {
        return Err(untracked_server(mcp_config_path, server_id));
    }
    let entry: OrderedJson = serde_json::from_value(entry)?;
    match servers.get_mut(server_id) {
        Some(existing) => *existing = entry,
        None => {
            servers.insert(server_id.to_string(), entry);
        }
    }
    if let Some(parent) = mcp_config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_ordered(mcp_config_path, &root)
}

/// `remove_tool` for an OpenCode config. An `mcp` object this call empties
/// is removed, and a file left holding only `$schema` is removed too.
fn remove_server_ordered(mcp_config_path: &Path, server_id: &str) -> Result<()> {
    let mut root = read_ordered(mcp_config_path)?;
    let OrderedJson::Object(object) = &mut root else {
        return Err(corrupt(mcp_config_path, "root must be a JSON object"));
    };
    let Some(OrderedJson::Object(servers)) = object.get_mut("mcp") else {
        return Ok(());
    };
    if servers.shift_remove(server_id).is_none() {
        return Ok(());
    }
    if servers.is_empty() {
        object.shift_remove("mcp");
    }
    if object.keys().all(|key| key == "$schema") {
        std::fs::remove_file(mcp_config_path)?;
        return Ok(());
    }
    write_ordered(mcp_config_path, &root)
}

fn read_ordered(mcp_config_path: &Path) -> Result<OrderedJson> {
    let raw = if mcp_config_path.exists() {
        std::fs::read_to_string(mcp_config_path)?
    } else {
        String::new()
    };
    if raw.trim().is_empty() {
        return Ok(OrderedJson::Object(indexmap::IndexMap::from([(
            "$schema".to_string(),
            OrderedJson::Scalar(serde_json::Value::String(OPENCODE_SCHEMA.to_string())),
        )])));
    }
    let root: OrderedJson =
        serde_json::from_str(&raw).map_err(|error| corrupt(mcp_config_path, &error.to_string()))?;
    let OrderedJson::Object(object) = &root else {
        return Err(corrupt(mcp_config_path, "root must be a JSON object"));
    };
    if object
        .get("mcp")
        .is_some_and(|servers| !matches!(servers, OrderedJson::Object(_)))
    {
        return Err(corrupt(
            mcp_config_path,
            "field 'mcp' must be a JSON object",
        ));
    }
    Ok(root)
}

fn write_ordered(mcp_config_path: &Path, root: &OrderedJson) -> Result<()> {
    std::fs::write(mcp_config_path, serde_json::to_string_pretty(root)? + "\n")?;
    Ok(())
}

fn corrupt(mcp_config_path: &Path, detail: &str) -> TuffError {
    TuffError::corrupt(format!(
        "invalid MCP config at {}: {detail}",
        mcp_config_path.display()
    ))
}

/// Whether `mcpServers.<id>` already exists. Used as a preflight so an
/// install can refuse *before* writing anything, rather than discovering the
/// collision after the capability's files are already on disk.
pub fn has_server(mcp_config_path: &Path, server_id: &str) -> Result<bool> {
    if !mcp_config_path.exists() {
        return Ok(false);
    }
    let config = read_config(mcp_config_path)?;
    Ok(config
        .get(servers_key(mcp_config_path))
        .and_then(serde_json::Value::as_object)
        .is_some_and(|servers| servers.contains_key(server_id)))
}

pub fn remove_tool(mcp_config_path: &Path, tool_id: &str) -> Result<()> {
    if !mcp_config_path.exists() {
        return Ok(());
    }
    if is_opencode(mcp_config_path) {
        return remove_server_ordered(mcp_config_path, tool_id);
    }

    let key = servers_key(mcp_config_path);
    let mut config = read_config(mcp_config_path)?;
    let Some(servers) = config
        .as_object_mut()
        .and_then(|object| object.get_mut(key))
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
                TuffError::corrupt(format!(
                    "invalid MCP config at {}: {error}",
                    mcp_config_path.display()
                ))
            })?
        }
    } else {
        serde_json::json!({})
    };

    let object = config.as_object().ok_or_else(|| {
        TuffError::corrupt(format!(
            "invalid MCP config at {}: root must be a JSON object",
            mcp_config_path.display()
        ))
    })?;
    let key = servers_key(mcp_config_path);
    if object.get(key).is_some_and(|servers| !servers.is_object()) {
        return Err(TuffError::corrupt(format!(
            "invalid MCP config at {}: field '{key}' must be a JSON object",
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
    fn an_opencode_config_keeps_its_key_order_and_loses_mcp_when_emptied() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".opencode").join("opencode.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Rule order is precedence in OpenCode: "*" must stay before "git push *".
        let original = "{\n  \"$schema\": \"https://opencode.ai/config.json\",\n  \"permission\": {\n    \"bash\": {\n      \"*\": \"allow\",\n      \"git push *\": \"allow\"\n    }\n  },\n  \"model\": \"x\"\n}\n";
        std::fs::write(&path, original).unwrap();

        register_server(
            &path,
            "everything",
            serde_json::json!({"type": "local", "command": ["npx", "-y", "pkg"]}),
            false,
        )
        .unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        let position = |needle: &str| raw.find(needle).unwrap_or_else(|| panic!("{needle}"));
        assert!(
            position("\"$schema\"") < position("\"permission\""),
            "{raw}"
        );
        assert!(
            position("\"*\": \"allow\"") < position("\"git push *\""),
            "{raw}"
        );
        assert!(
            position("\"model\"") < position("\"mcp\""),
            "new keys go last: {raw}"
        );
        assert!(!raw.contains("mcpServers"), "{raw}");
        assert!(has_server(&path, "everything").unwrap());

        let error = register_server(&path, "everything", serde_json::json!({}), false)
            .expect_err("untracked entry");
        assert!(
            error.to_string().contains("refusing to overwrite"),
            "{error}"
        );

        remove_tool(&path, "everything").unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(
            !after.contains("\"mcp\""),
            "an emptied mcp object is removed: {after}"
        );
        assert!(after.contains("\"model\": \"x\""), "{after}");

        // A file Tuff created holds only $schema once its last server goes.
        let fresh = temp.path().join("fresh").join("opencode.json");
        register_server(&fresh, "s", serde_json::json!({"type": "local"}), false).unwrap();
        let created: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&fresh).unwrap()).unwrap();
        assert_eq!(created["$schema"], "https://opencode.ai/config.json");
        remove_tool(&fresh, "s").unwrap();
        assert!(!fresh.exists());
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

    #[test]
    fn register_server_writes_entry_and_preserves_neighbours() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("mcp.json");
        std::fs::write(
            &path,
            "{\"custom\":true,\"mcpServers\":{\"other\":{\"command\":\"x\"}}}",
        )
        .unwrap();

        register_server(
            &path,
            "github",
            serde_json::json!({"command": "npx", "args": ["-y", "srv"], "env": {"T": "${T}"}}),
            false,
        )
        .unwrap();

        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(config["custom"], true);
        assert_eq!(config["mcpServers"]["other"]["command"], "x");
        assert_eq!(config["mcpServers"]["github"]["env"]["T"], "${T}");
        assert!(has_server(&path, "github").unwrap());
        assert!(!has_server(&path, "missing").unwrap());
    }

    #[test]
    fn register_server_refuses_untracked_collision_unless_overwrite_allowed() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("mcp.json");
        let original = "{\"mcpServers\":{\"github\":{\"command\":\"hand\"}}}";
        std::fs::write(&path, original).unwrap();

        let error = register_server(
            &path,
            "github",
            serde_json::json!({"command": "npx"}),
            false,
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("refusing to overwrite untracked MCP server"),
            "{error}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);

        register_server(&path, "github", serde_json::json!({"command": "npx"}), true).unwrap();
        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(config["mcpServers"]["github"]["command"], "npx");
    }
}
