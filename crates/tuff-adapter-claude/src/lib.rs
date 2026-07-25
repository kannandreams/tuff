use std::path::Path;

use tuff_hooks_spec::{
    CompatibilityEntry, CompatibilityMatrix, CoverageLevel, HookEvent, SPEC_VERSION,
};

use tuff_core::adapter::AgentAdapter;
use tuff_core::manifest::{CapabilityType, HookConfig};
use tuff_core::{
    error::{Result, TuffError},
    lockfile,
};

pub const ID: &str = "claude";
pub const DISPLAY_NAME: &str = "Claude";
pub const SETTINGS_RELPATH: &str = ".claude/settings.json";
pub struct Claude;
pub const SUPPORTED_TYPES: &[CapabilityType] = &[
    CapabilityType::Skill,
    CapabilityType::Tool,
    CapabilityType::Hook,
    CapabilityType::Workflow,
];

pub const SUPPORTED_AGENTS: &[&str] = &["Claude Code"];

pub const HOOK_COMPATIBILITY: CompatibilityMatrix = CompatibilityMatrix {
    spec_version: SPEC_VERSION,
    adapter: ID,
    events: &[
        CompatibilityEntry {
            event: HookEvent::SessionStart,
            native_event: Some("SessionStart"),
            aliases: &["SessionStart"],
            coverage: CoverageLevel::Full,
            scope: &["startup", "resume"],
            caveat: None,
            source: None,
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::BeforeFinish,
            native_event: Some("before_finish"),
            aliases: &[],
            coverage: CoverageLevel::Full,
            scope: &[],
            caveat: None,
            source: None,
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::PostToolUse,
            native_event: Some("post_tool_execution"),
            aliases: &["post_tool_execution"],
            coverage: CoverageLevel::Full,
            scope: &["tool calls"],
            caveat: None,
            source: None,
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::PreToolUse,
            native_event: None,
            aliases: &["pre_tool_execution"],
            coverage: CoverageLevel::Unsupported,
            scope: &[],
            caveat: Some(
                "Claude adapter support for PreToolUse rendering has not been implemented in Tuff yet.",
            ),
            source: None,
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::AfterSave,
            native_event: None,
            aliases: &[],
            coverage: CoverageLevel::Unsupported,
            scope: &[],
            caveat: Some(
                "Claude adapter support for after-save rendering has not been implemented in Tuff yet.",
            ),
            source: None,
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::SessionEnd,
            native_event: None,
            aliases: &[],
            coverage: CoverageLevel::Unsupported,
            scope: &[],
            caveat: Some(
                "Claude adapter support for session-end rendering has not been implemented in Tuff yet.",
            ),
            source: None,
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::Stop,
            native_event: None,
            aliases: &[],
            coverage: CoverageLevel::Unsupported,
            scope: &[],
            caveat: Some(
                "Claude adapter support for stop rendering has not been implemented in Tuff yet.",
            ),
            source: None,
            since_harness_version: None,
            until_harness_version: None,
        },
    ],
};

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
        .ok_or_else(|| TuffError::new(".claude/settings.json must be a JSON object"))?;

    let fragment_hooks = fragment
        .get("hooks")
        .and_then(|hooks| hooks.as_object())
        .ok_or_else(|| TuffError::new("--hook-file fragment must contain a 'hooks' object"))?;

    let settings_hooks = settings_obj
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    let settings_hooks = settings_hooks
        .as_object_mut()
        .ok_or_else(|| TuffError::new(".claude/settings.json field 'hooks' must be an object"))?;

    for (event, additions) in fragment_hooks {
        let additions = additions.as_array().ok_or_else(|| {
            TuffError::new(format!(
                "--hook-file hooks.{event} must be an array of hook groups"
            ))
        })?;
        let existing_event = settings_hooks
            .entry(event.clone())
            .or_insert_with(|| serde_json::json!([]));
        let existing_event = existing_event.as_array_mut().ok_or_else(|| {
            TuffError::new(format!(
                ".claude/settings.json hooks.{event} must be an array"
            ))
        })?;
        existing_event.extend(additions.iter().cloned());
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

    let mut empty_events = Vec::new();
    for (event, groups) in hooks.iter_mut() {
        let Some(groups) = groups.as_array_mut() else {
            continue;
        };
        let registrations: Vec<&lockfile::ManagedHook> = managed_hooks
            .iter()
            .filter(|hook| hook.settings_path == SETTINGS_RELPATH && hook.event == *event)
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
        .ok_or_else(|| TuffError::new("--hook-file fragment must be a JSON object"))?;
    if !obj.contains_key("hooks") {
        return Err(TuffError::new(
            "--hook-file fragment must contain a top-level 'hooks' object",
        ));
    }
    if obj.keys().any(|key| key != "hooks") {
        return Err(TuffError::new(
            "--hook-file must be a hooks-only fragment, not a full settings.json",
        ));
    }
    if !fragment["hooks"].is_object() {
        return Err(TuffError::new(
            "--hook-file field 'hooks' must be an object",
        ));
    }
    Ok(())
}

impl AgentAdapter for Claude {
    fn id(&self) -> &'static str {
        ID
    }

    fn display_name(&self) -> &'static str {
        DISPLAY_NAME
    }

    fn dir_prefix(&self) -> &'static str {
        ".claude"
    }

    fn mcp_config_relpath(&self) -> &'static str {
        ".mcp.json"
    }

    fn supported_agents(&self) -> &[&'static str] {
        SUPPORTED_AGENTS
    }

    fn kinds_supported(&self) -> &[CapabilityType] {
        SUPPORTED_TYPES
    }

    fn hook_compatibility(&self) -> &'static CompatibilityMatrix {
        &HOOK_COMPATIBILITY
    }

    fn hook_settings_relpath(&self) -> &'static str {
        SETTINGS_RELPATH
    }

    fn scaffold_hook_event(&self) -> &'static str {
        "SessionStart"
    }

    fn hook_filename(&self) -> &'static str {
        "run.sh"
    }

    fn hook_file_content(&self, hook_cfg: &HookConfig) -> Result<Vec<u8>> {
        Ok(format!(
            "#!/usr/bin/env bash\nset -euo pipefail\ncd \"{}\"\n{}\n",
            hook_cfg.working_directory, hook_cfg.command
        )
        .into_bytes())
    }

    fn command_hook_fragment(&self, native_event: &str, command: &str) -> serde_json::Value {
        serde_json::json!({
            "hooks": {
                native_event: [{
                    "hooks": [{"type": "command", "command": command}]
                }]
            }
        })
    }

    fn merge_hook_fragment(
        &self,
        existing: Option<&[u8]>,
        fragment: &serde_json::Value,
    ) -> Result<Vec<u8>> {
        merge_hook_fragment(existing, fragment)
    }

    fn remove_hook_settings(
        &self,
        repo_root: &Path,
        managed_hooks: &[lockfile::ManagedHook],
    ) -> Result<()> {
        remove_hook_settings(repo_root, managed_hooks)
    }

    fn detect(&self, repo_root: &Path) -> bool {
        detect(repo_root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_not_empty() {
        assert!(!ID.is_empty());
        assert!(!DISPLAY_NAME.is_empty());
        assert!(!HOOK_COMPATIBILITY.events.is_empty());
    }

    #[test]
    fn supported_types_covers_all_capability_types() {
        assert_eq!(SUPPORTED_TYPES.len(), 4);
    }
}
