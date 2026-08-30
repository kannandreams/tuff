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

pub const ID: &str = "claude";
pub const DISPLAY_NAME: &str = "Claude";
pub const SETTINGS_RELPATH: &str = ".claude/settings.json";
pub struct Claude;
pub const SUPPORTED_TYPES: &[CapabilityType] = &[
    CapabilityType::Skill,
    CapabilityType::Tool,
    CapabilityType::Hook,
    CapabilityType::Workflow,
    CapabilityType::McpServer,
];

pub const SUPPORTED_AGENTS: &[&str] = &["Claude Code"];
const CLAUDE_HOOKS_DOCS: &str = "https://code.claude.com/docs/en/hooks";

pub const HOOK_COMPATIBILITY: CompatibilityMatrix = CompatibilityMatrix {
    spec_version: SPEC_VERSION,
    adapter: ID,
    events: &[
        CompatibilityEntry {
            event: HookEvent::SessionStart,
            native_event: Some("SessionStart"),
            aliases: &["SessionStart"],
            coverage: CoverageLevel::Full,
            scope: &["startup", "resume", "clear", "compact", "fork"],
            caveat: None,
            source: Some(CLAUDE_HOOKS_DOCS),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::SessionEnd,
            native_event: Some("SessionEnd"),
            aliases: &["SessionEnd"],
            coverage: CoverageLevel::Full,
            scope: &["session lifecycle"],
            caveat: None,
            source: Some(CLAUDE_HOOKS_DOCS),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::PreToolUse,
            native_event: Some("PreToolUse"),
            aliases: &["PreToolUse"],
            coverage: CoverageLevel::Full,
            scope: &["tool calls"],
            caveat: None,
            source: Some(CLAUDE_HOOKS_DOCS),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::PostToolUse,
            native_event: Some("PostToolUse"),
            aliases: &["PostToolUse"],
            coverage: CoverageLevel::Full,
            scope: &["successful tool calls"],
            caveat: None,
            source: Some(CLAUDE_HOOKS_DOCS),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::BeforeFinish,
            native_event: Some("Stop"),
            aliases: &[],
            coverage: CoverageLevel::Partial,
            scope: &["main-agent completion"],
            caveat: Some(
                "Claude Stop runs after the main agent finishes responding and can request continuation; it does not represent every possible pre-finish boundary.",
            ),
            source: Some(CLAUDE_HOOKS_DOCS),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::AfterSave,
            native_event: None,
            aliases: &["FileChanged"],
            coverage: CoverageLevel::Unsupported,
            scope: &[],
            caveat: Some(
                "Claude FileChanged requires watched filenames or paths that Tuff's standard after_save hook cannot currently express.",
            ),
            source: Some(CLAUDE_HOOKS_DOCS),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::Stop,
            native_event: Some("Stop"),
            aliases: &["Stop"],
            coverage: CoverageLevel::Full,
            scope: &["main-agent completion"],
            caveat: None,
            source: Some(CLAUDE_HOOKS_DOCS),
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
        extend_hook_groups(existing_event, additions);
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
        assert_eq!(SUPPORTED_TYPES.len(), 5);
    }

    #[test]
    fn hook_matrix_matches_claude_native_event_contract() {
        let actual: Vec<_> = HOOK_COMPATIBILITY
            .events
            .iter()
            .map(|entry| {
                (
                    entry.event,
                    entry.native_event,
                    entry.aliases,
                    entry.coverage,
                )
            })
            .collect();

        assert_eq!(
            actual,
            vec![
                (
                    HookEvent::SessionStart,
                    Some("SessionStart"),
                    &["SessionStart"][..],
                    CoverageLevel::Full
                ),
                (
                    HookEvent::SessionEnd,
                    Some("SessionEnd"),
                    &["SessionEnd"][..],
                    CoverageLevel::Full
                ),
                (
                    HookEvent::PreToolUse,
                    Some("PreToolUse"),
                    &["PreToolUse"][..],
                    CoverageLevel::Full
                ),
                (
                    HookEvent::PostToolUse,
                    Some("PostToolUse"),
                    &["PostToolUse"][..],
                    CoverageLevel::Full
                ),
                (
                    HookEvent::BeforeFinish,
                    Some("Stop"),
                    &[][..],
                    CoverageLevel::Partial
                ),
                (
                    HookEvent::AfterSave,
                    None,
                    &["FileChanged"][..],
                    CoverageLevel::Unsupported
                ),
                (
                    HookEvent::Stop,
                    Some("Stop"),
                    &["Stop"][..],
                    CoverageLevel::Full
                ),
            ]
        );
    }

    #[test]
    fn merging_the_same_fragment_twice_does_not_duplicate_the_hook() {
        let fragment = serde_json::json!({
            "hooks": {
                "PreToolUse": [{"hooks": [{"type": "command", "command": "sh .claude/hooks/demo/run.sh"}]}]
            }
        });

        let once = merge_hook_fragment(None, &fragment).expect("first merge");
        let twice = merge_hook_fragment(Some(&once), &fragment).expect("second merge");

        let settings: serde_json::Value = serde_json::from_slice(&twice).expect("valid json");
        let groups = settings["hooks"]["PreToolUse"]
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
