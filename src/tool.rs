use std::path::Path;

use crate::error::{CoralError, Result};

pub fn validate_json_schema(value: &serde_json::Value) -> Result<()> {
    let obj = value.as_object().ok_or_else(|| {
        CoralError::new("parameters must be a JSON object with 'type: object'")
    })?;

    let schema_type = obj
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if schema_type != "object" {
        return Err(CoralError::new(format!(
            "parameters 'type' must be 'object', got '{}'",
            schema_type
        )));
    }

    if !obj.contains_key("properties") {
        return Err(CoralError::new(
            "parameters must have a 'properties' section defining the tool's input schema",
        ));
    }

    let properties = obj.get("properties").and_then(|v| v.as_object());
    if properties.is_none() || properties.unwrap().is_empty() {
        return Err(CoralError::new(
            "parameters 'properties' must contain at least one parameter definition",
        ));
    }

    if obj.contains_key("required") {
        let required = obj.get("required").and_then(|v| v.as_array());
        if required.is_none() {
            return Err(CoralError::new("parameters 'required' must be an array of field names"));
        }
    }

    Ok(())
}

pub fn validate_entrypoint(primitive_dir: &Path, entrypoint: &str) -> Result<()> {
    check_path_traversal(entrypoint)?;

    let path = primitive_dir.join(entrypoint);
    if !path.exists() {
        return Err(CoralError::new(format!(
            "implementation entrypoint not found: {}",
            path.display()
        )));
    }

    if !path.is_file() {
        return Err(CoralError::new(format!(
            "implementation entrypoint must be a file, not a directory: {}",
            path.display()
        )));
    }

    Ok(())
}

pub fn check_path_traversal(entrypoint: &str) -> Result<()> {
    if entrypoint.is_empty() {
        return Err(CoralError::new("implementation entrypoint must not be empty"));
    }

    if entrypoint.starts_with('/') {
        return Err(CoralError::new(
            "implementation entrypoint must be a relative path, not absolute",
        ));
    }

    let clean = entrypoint.trim_start_matches("./");

    for component in clean.split('/') {
        if component == ".." {
            return Err(CoralError::new(
                "implementation entrypoint must not use '..' — path traversal is not allowed",
            ));
        }
    }

    Ok(())
}
