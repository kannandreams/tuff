use std::path::Path;

use tuff_hooks_spec::{
    CompatibilityEntry, CompatibilityMatrix, CoverageLevel, HookEvent, SPEC_VERSION,
};

use tuff_core::adapter::{AgentAdapter, HookSettingsShape};
use tuff_core::manifest::CapabilityType;

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

    fn hook_settings_shape(&self) -> HookSettingsShape {
        HookSettingsShape::Grouped
    }

    fn detect(&self, repo_root: &Path) -> bool {
        repo_root.join(".claude").exists() || repo_root.join("CLAUDE.md").exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC-106 D2: Claude Code keeps `"type": "http"` and expands `${VAR}`.
    #[test]
    fn a_remote_server_entry_declares_type_and_renders_headers() {
        let server = tuff_core::manifest::McpServerConfig {
            transport: tuff_core::manifest::McpTransport::Http,
            command: None,
            args: Vec::new(),
            url: Some("https://mcp.example.test/mcp".to_string()),
            env: Default::default(),
            headers: [
                (
                    "Authorization".to_string(),
                    tuff_core::manifest::HeaderRef {
                        from_env: "EXAMPLE_TOKEN".to_string(),
                        format: Some("Bearer {}".to_string()),
                    },
                ),
                (
                    "X-Api-Key".to_string(),
                    tuff_core::manifest::HeaderRef {
                        from_env: "EXAMPLE_KEY".to_string(),
                        format: None,
                    },
                ),
            ]
            .into_iter()
            .collect(),
            metadata: None,
        };

        let entry = Claude.mcp_server_entry(&server);

        assert_eq!(
            entry,
            serde_json::json!({
                "type": "http",
                "url": "https://mcp.example.test/mcp",
                "headers": {
                    "Authorization": "Bearer ${EXAMPLE_TOKEN}",
                    "X-Api-Key": "${EXAMPLE_KEY}",
                },
            })
        );
    }

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

        let once = Claude
            .merge_hook_fragment(None, &fragment)
            .expect("first merge");
        let twice = Claude
            .merge_hook_fragment(Some(&once), &fragment)
            .expect("second merge");

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
