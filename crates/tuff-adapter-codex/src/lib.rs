use std::path::Path;

use tuff_hooks_spec::{
    CompatibilityEntry, CompatibilityMatrix, CoverageLevel, HookEvent, SPEC_VERSION,
};

use tuff_core::adapter::{AgentAdapter, extend_hook_groups};
use tuff_core::manifest::CapabilityType;
use tuff_core::{
    error::{Result, TuffError},
    lockfile,
};

pub const ID: &str = "codex";
pub const DISPLAY_NAME: &str = "Codex";
pub const SUPPORTED_TYPES: &[CapabilityType] = &[
    CapabilityType::Skill,
    CapabilityType::Tool,
    CapabilityType::Hook,
    CapabilityType::Workflow,
];

pub const SUPPORTED_AGENTS: &[&str] = &["Codex"];

pub const HOOK_SETTINGS_RELPATH: &str = ".agents/hook.json";

pub struct Codex;

pub const HOOK_COMPATIBILITY: CompatibilityMatrix = CompatibilityMatrix {
    spec_version: SPEC_VERSION,
    adapter: ID,
    events: &[
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
            event: HookEvent::AfterSave,
            native_event: Some("after_save"),
            aliases: &[],
            coverage: CoverageLevel::Full,
            scope: &[],
            caveat: None,
            source: None,
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::PreToolUse,
            native_event: Some("pre_tool_execution"),
            aliases: &["pre_tool_execution"],
            coverage: CoverageLevel::Partial,
            scope: &["local function tools", "Bash", "Edit", "Write", "MCP"],
            caveat: Some("Codex hosted tools do not use the local function-tool hook path."),
            source: Some("https://learn.chatgpt.com/docs/hooks.md"),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::PostToolUse,
            native_event: Some("post_tool_execution"),
            aliases: &["post_tool_execution"],
            coverage: CoverageLevel::Partial,
            scope: &["local function tools", "Bash", "Edit", "Write", "MCP"],
            caveat: Some("Codex hosted tools do not use the local function-tool hook path."),
            source: Some("https://learn.chatgpt.com/docs/hooks.md"),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::SessionStart,
            native_event: None,
            aliases: &[],
            coverage: CoverageLevel::Unsupported,
            scope: &[],
            caveat: Some("Codex hook.json does not currently define a session-start event."),
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
            caveat: Some("Codex hook.json does not currently define a session-end event."),
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
            caveat: Some("Codex hook.json does not currently define a stop event."),
            source: None,
            since_harness_version: None,
            until_harness_version: None,
        },
    ],
};

pub fn detect(repo_root: &Path) -> bool {
    repo_root.join(".agents").exists() || repo_root.join("AGENTS.md").exists()
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
        .ok_or_else(|| TuffError::new(".agents/hook.json must be a JSON object"))?;
    let fragment_hooks = fragment["hooks"]
        .as_object()
        .expect("validated hooks object");
    let settings_hooks = settings_obj
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| TuffError::new(".agents/hook.json field 'hooks' must be an object"))?;
    for (event, additions) in fragment_hooks {
        let additions = additions
            .as_array()
            .ok_or_else(|| TuffError::new(format!("--hook-file hooks.{event} must be an array")))?;
        let groups = settings_hooks
            .entry(event.clone())
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .ok_or_else(|| {
                TuffError::new(format!(".agents/hook.json hooks.{event} must be an array"))
            })?;
        extend_hook_groups(groups, additions);
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
        "updated Codex hook settings -> {}",
        lockfile::relative_or_absolute_fs(&settings_path, repo_root)
    );
    Ok(())
}

fn validate_hook_fragment(fragment: &serde_json::Value) -> Result<()> {
    let obj = fragment
        .as_object()
        .ok_or_else(|| TuffError::new("--hook-file fragment must be a JSON object"))?;
    if !obj.contains_key("hooks") || obj.keys().any(|key| key != "hooks") {
        return Err(TuffError::new("--hook-file must be a hooks-only fragment"));
    }
    if !fragment["hooks"].is_object() {
        return Err(TuffError::new(
            "--hook-file field 'hooks' must be an object",
        ));
    }
    Ok(())
}

impl AgentAdapter for Codex {
    fn id(&self) -> &'static str {
        ID
    }

    fn display_name(&self) -> &'static str {
        DISPLAY_NAME
    }

    fn dir_prefix(&self) -> &'static str {
        ".agents"
    }

    fn mcp_config_relpath(&self) -> &'static str {
        ".agents/mcp.json"
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
        HOOK_SETTINGS_RELPATH
    }

    fn scaffold_hook_event(&self) -> &'static str {
        "before_finish"
    }

    fn hook_filename(&self) -> &'static str {
        "run.sh"
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
    fn id_and_display_name_are_not_empty() {
        assert!(!ID.is_empty());
        assert!(!DISPLAY_NAME.is_empty());
    }

    #[test]
    fn supported_types_covers_all_capability_types() {
        assert_eq!(SUPPORTED_TYPES.len(), 4);
    }

    #[test]
    fn merging_the_same_fragment_twice_does_not_duplicate_the_hook() {
        let fragment = serde_json::json!({
            "hooks": {
                "before_finish": [{"hooks": [{"type": "command", "command": "sh .agents/hooks/demo/run.sh"}]}]
            }
        });

        let once = merge_hook_fragment(None, &fragment).expect("first merge");
        let twice = merge_hook_fragment(Some(&once), &fragment).expect("second merge");

        let settings: serde_json::Value = serde_json::from_slice(&twice).expect("valid json");
        let groups = settings["hooks"]["before_finish"]
            .as_array()
            .expect("event array");
        assert_eq!(
            groups.len(),
            1,
            "re-adding a hook must not register it twice"
        );
        assert_eq!(
            once, twice,
            "a redundant merge must leave the file unchanged"
        );
    }
}
