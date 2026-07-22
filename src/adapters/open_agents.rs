use std::path::Path;

use crate::manifest::CapabilityType;
use crate::{
    error::{CoralError, Result},
    lockfile,
};

pub const ID: &str = "open-agents";
pub const DISPLAY_NAME: &str = "Open Agents";
pub const SUPPORTED_TYPES: &[CapabilityType] = &[
    CapabilityType::Skill,
    CapabilityType::Tool,
    CapabilityType::Hook,
    CapabilityType::Workflow,
];

pub const SUPPORTED_AGENTS: &[&str] = &[
    "Codex",
    "Cursor",
    "OpenCode",
    "GitHub Copilot",
    "Gemini CLI",
    "Roo",
    "Cline",
    "Windsurf",
];

pub const SUPPORTED_EVENTS: &[&str] = &[
    "before_finish",
    "after_save",
    "pre_tool_execution",
    "post_tool_execution",
];
pub const HOOK_SETTINGS_RELPATH: &str = ".agents/hook.json";

pub fn detect(repo_root: &Path) -> bool {
    repo_root.join(".agents").join("skills").exists()
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
        .ok_or_else(|| CoralError::new(".agents/hook.json must be a JSON object"))?;
    let fragment_hooks = fragment["hooks"]
        .as_object()
        .expect("validated hooks object");
    let settings_hooks = settings_obj
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| CoralError::new(".agents/hook.json field 'hooks' must be an object"))?;
    for (event, additions) in fragment_hooks {
        let additions = additions.as_array().ok_or_else(|| {
            CoralError::new(format!("--hook-file hooks.{event} must be an array"))
        })?;
        settings_hooks
            .entry(event.clone())
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .ok_or_else(|| {
                CoralError::new(format!(".agents/hook.json hooks.{event} must be an array"))
            })?
            .extend(additions.iter().cloned());
    }
    Ok(serde_json::to_string_pretty(&settings)?.into_bytes())
}

pub fn remove_hook_settings(
    repo_root: &Path,
    managed_hooks: &[lockfile::ManagedHook],
) -> Result<()> {
    if managed_hooks.is_empty() {
        return Ok(());
    }
    let settings_path = repo_root.join(HOOK_SETTINGS_RELPATH);
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
    let mut empty_events = Vec::new();
    for (event, groups) in hooks.iter_mut() {
        if let Some(groups) = groups.as_array_mut() {
            let registrations: Vec<&lockfile::ManagedHook> = managed_hooks
                .iter()
                .filter(|hook| hook.settings_path == HOOK_SETTINGS_RELPATH && hook.event == *event)
                .collect();
            for group in groups.iter_mut() {
                if let Some(entries) = group
                    .get_mut("hooks")
                    .and_then(|value| value.as_array_mut())
                {
                    entries.retain(|entry| {
                        !registrations.iter().any(|hook| {
                            entry.get("command").and_then(serde_json::Value::as_str)
                                == Some(hook.command.as_str())
                        })
                    });
                }
            }
            groups.retain(|group| {
                group
                    .get("hooks")
                    .and_then(|value| value.as_array())
                    .is_none_or(|entries| !entries.is_empty())
            });
            if groups.is_empty() {
                empty_events.push(event.clone());
            }
        }
    }
    for event in empty_events {
        hooks.remove(&event);
    }
    std::fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings)? + "\n",
    )?;
    eprintln!(
        "updated Open Agents hook settings -> {}",
        lockfile::relative_or_absolute_fs(&settings_path, repo_root)
    );
    Ok(())
}

fn validate_hook_fragment(fragment: &serde_json::Value) -> Result<()> {
    let obj = fragment
        .as_object()
        .ok_or_else(|| CoralError::new("--hook-file fragment must be a JSON object"))?;
    if !obj.contains_key("hooks") || obj.keys().any(|key| key != "hooks") {
        return Err(CoralError::new("--hook-file must be a hooks-only fragment"));
    }
    if !fragment["hooks"].is_object() {
        return Err(CoralError::new(
            "--hook-file field 'hooks' must be an object",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_and_display_name_are_not_empty() {
        assert!(!ID.is_empty());
        assert!(!DISPLAY_NAME.is_empty());
    }

    #[test]
    fn supported_types_covers_all_capability_types() {
        assert_eq!(SUPPORTED_TYPES.len(), 4);
    }
}
