use std::path::Path;

use crate::error::{Result, TuffError};

pub fn validate_json_schema(value: &serde_json::Value) -> Result<()> {
    let obj = value
        .as_object()
        .ok_or_else(|| TuffError::new("parameters must be a JSON object with 'type: object'"))?;

    let schema_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if schema_type != "object" {
        return Err(TuffError::new(format!(
            "parameters 'type' must be 'object', got '{}'",
            schema_type
        )));
    }

    if !obj.contains_key("properties") {
        return Err(TuffError::new(
            "parameters must have a 'properties' section defining the tool's input schema",
        ));
    }

    let properties = obj.get("properties").and_then(|v| v.as_object());
    if properties.is_none() || properties.unwrap().is_empty() {
        return Err(TuffError::new(
            "parameters 'properties' must contain at least one parameter definition",
        ));
    }

    if obj.contains_key("required") {
        let required = obj.get("required").and_then(|v| v.as_array());
        if required.is_none() {
            return Err(TuffError::new(
                "parameters 'required' must be an array of field names",
            ));
        }
    }

    Ok(())
}

pub fn validate_entrypoint(primitive_dir: &Path, entrypoint: &str) -> Result<()> {
    check_path_traversal(entrypoint)?;

    let path = primitive_dir.join(entrypoint);
    if !path.exists() {
        return Err(TuffError::new(format!(
            "implementation entrypoint not found: {}",
            path.display()
        )));
    }

    if !path.is_file() {
        return Err(TuffError::new(format!(
            "implementation entrypoint must be a file, not a directory: {}",
            path.display()
        )));
    }

    Ok(())
}

pub fn check_path_traversal(entrypoint: &str) -> Result<()> {
    if entrypoint.is_empty() {
        return Err(TuffError::new(
            "implementation entrypoint must not be empty",
        ));
    }

    if entrypoint.starts_with('/') {
        return Err(TuffError::new(
            "implementation entrypoint must be a relative path, not absolute",
        ));
    }

    let clean = entrypoint.trim_start_matches("./");

    for component in clean.split('/') {
        if component == ".." {
            return Err(TuffError::new(
                "implementation entrypoint must not use '..' — path traversal is not allowed",
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn schema_valid_object_with_properties() {
        let schema = json!({
            "type": "object",
            "properties": { "target": { "type": "string" } },
            "required": ["target"]
        });
        assert!(validate_json_schema(&schema).is_ok());
    }

    #[test]
    fn schema_rejects_missing_type() {
        assert!(validate_json_schema(&json!({"properties": {"x": {"type": "string"}}})).is_err());
    }

    #[test]
    fn schema_rejects_non_object_type() {
        assert!(validate_json_schema(&json!({"type": "string", "properties": {}})).is_err());
    }

    #[test]
    fn schema_rejects_missing_properties() {
        assert!(validate_json_schema(&json!({"type": "object"})).is_err());
    }

    #[test]
    fn schema_rejects_empty_properties() {
        assert!(validate_json_schema(&json!({"type": "object", "properties": {}})).is_err());
    }

    #[test]
    fn schema_accepts_no_required_field() {
        assert!(
            validate_json_schema(
                &json!({"type": "object", "properties": {"file": {"type": "string"}}})
            )
            .is_ok()
        );
    }

    #[test]
    fn path_traversal_blocks_dot_dot() {
        assert!(check_path_traversal("../etc/passwd").is_err());
        assert!(check_path_traversal("scripts/../../../etc/passwd").is_err());
    }

    #[test]
    fn path_traversal_blocks_absolute() {
        assert!(check_path_traversal("/etc/passwd").is_err());
    }

    #[test]
    fn path_traversal_blocks_empty() {
        assert!(check_path_traversal("").is_err());
    }

    #[test]
    fn path_traversal_allows_relative() {
        assert!(check_path_traversal("run.sh").is_ok());
        assert!(check_path_traversal("./run.sh").is_ok());
        assert!(check_path_traversal("scripts/run.sh").is_ok());
    }

    #[test]
    fn entrypoint_rejects_non_existent_file() {
        let tmp = TempDir::new().unwrap();
        assert!(validate_entrypoint(tmp.path(), "missing.sh").is_err());
    }

    #[test]
    fn entrypoint_accepts_valid_file() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("run.sh"), "echo ok").unwrap();
        assert!(validate_entrypoint(tmp.path(), "run.sh").is_ok());
    }

    #[test]
    fn entrypoint_rejects_directory() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("subdir")).unwrap();
        assert!(validate_entrypoint(tmp.path(), "subdir").is_err());
    }
}
