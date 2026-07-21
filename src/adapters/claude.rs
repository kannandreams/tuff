use std::path::Path;

use crate::manifest::CapabilityType;
use crate::{
    error::{CoralError, Result},
    lockfile,
};

pub const ID: &str = "claude";
pub const DISPLAY_NAME: &str = "Claude";
pub const SETTINGS_RELPATH: &str = ".claude/settings.json";
pub const SUPPORTED_TYPES: &[CapabilityType] = &[
    CapabilityType::Skill,
    CapabilityType::Tool,
    CapabilityType::Hook,
    CapabilityType::Workflow,
];

pub const SUPPORTED_AGENTS: &[&str] = &["Claude Code"];

pub const SUPPORTED_EVENTS: &[&str] = &["before_finish", "post_tool_execution"];

pub fn detect(repo_root: &Path) -> bool {
    repo_root.join(".claude").exists() || repo_root.join("CLAUDE.md").exists()
}

pub fn merge_hook_fragment(
    existing: Option<&[u8]>,
    fragment: &serde_json::Value,
) -> Result<Vec<u8>> {
    validate_hook_fragment(fragment)?;

    let mut settings = match existing {
        Some(bytes) if !bytes.is_empty() => serde_json::from_slice(bytes)?,
        _ => serde_json::json!({}),
    };
    let settings_obj = settings
        .as_object_mut()
        .ok_or_else(|| CoralError::new(".claude/settings.json must be a JSON object"))?;

    let fragment_hooks = fragment
        .get("hooks")
        .and_then(|hooks| hooks.as_object())
        .ok_or_else(|| CoralError::new("--hook-file fragment must contain a 'hooks' object"))?;

    let settings_hooks = settings_obj
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    let settings_hooks = settings_hooks
        .as_object_mut()
        .ok_or_else(|| CoralError::new(".claude/settings.json field 'hooks' must be an object"))?;

    for (event, additions) in fragment_hooks {
        let additions = additions.as_array().ok_or_else(|| {
            CoralError::new(format!(
                "--hook-file hooks.{event} must be an array of hook groups"
            ))
        })?;
        let existing_event = settings_hooks
            .entry(event.clone())
            .or_insert_with(|| serde_json::json!([]));
        let existing_event = existing_event.as_array_mut().ok_or_else(|| {
            CoralError::new(format!(
                ".claude/settings.json hooks.{event} must be an array"
            ))
        })?;
        existing_event.extend(additions.iter().cloned());
    }

    Ok(serde_json::to_string_pretty(&settings)?.into_bytes())
}

pub fn remove_hook_settings(repo_root: &Path, hook_id: &str) -> Result<()> {
    let settings_path = repo_root.join(SETTINGS_RELPATH);
    if !settings_path.is_file() {
        return Ok(());
    }

    let mut settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path)?)?;
    let Some(hooks) = settings
        .get_mut("hooks")
        .and_then(|hooks| hooks.as_object_mut())
    else {
        return Ok(());
    };

    let marker = format!(".claude/hooks/{hook_id}/");
    let mut empty_events = Vec::new();
    for (event, groups) in hooks.iter_mut() {
        let Some(groups) = groups.as_array_mut() else {
            continue;
        };
        groups.retain(|group| !contains_command_marker(group, &marker));
        if groups.is_empty() {
            empty_events.push(event.clone());
        }
    }
    for event in empty_events {
        hooks.remove(&event);
    }

    std::fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings)? + "\n",
    )?;
    let rel = lockfile::relative_or_absolute_fs(&settings_path, repo_root);
    eprintln!("updated Claude hook settings -> {rel}");
    Ok(())
}

fn validate_hook_fragment(fragment: &serde_json::Value) -> Result<()> {
    let obj = fragment
        .as_object()
        .ok_or_else(|| CoralError::new("--hook-file fragment must be a JSON object"))?;
    if !obj.contains_key("hooks") {
        return Err(CoralError::new(
            "--hook-file fragment must contain a top-level 'hooks' object",
        ));
    }
    if obj.keys().any(|key| key != "hooks") {
        return Err(CoralError::new(
            "--hook-file must be a hooks-only fragment, not a full settings.json",
        ));
    }
    if !fragment["hooks"].is_object() {
        return Err(CoralError::new(
            "--hook-file field 'hooks' must be an object",
        ));
    }
    Ok(())
}

fn contains_command_marker(value: &serde_json::Value, marker: &str) -> bool {
    match value {
        serde_json::Value::String(s) => s.contains(marker),
        serde_json::Value::Array(items) => items
            .iter()
            .any(|item| contains_command_marker(item, marker)),
        serde_json::Value::Object(map) => map
            .values()
            .any(|item| contains_command_marker(item, marker)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_not_empty() {
        assert!(!ID.is_empty());
        assert!(!DISPLAY_NAME.is_empty());
        assert!(!SUPPORTED_EVENTS.is_empty());
    }

    #[test]
    fn supported_types_covers_all_capability_types() {
        assert_eq!(SUPPORTED_TYPES.len(), 4);
    }
}
