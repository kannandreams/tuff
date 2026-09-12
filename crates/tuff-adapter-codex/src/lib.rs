use std::path::Path;

use tuff_hooks_spec::{
    CompatibilityEntry, CompatibilityMatrix, CoverageLevel, HookEvent, SPEC_VERSION,
};

use tuff_core::adapter::{AgentAdapter, HookSettingsShape};
use tuff_core::manifest::CapabilityType;

pub const ID: &str = "codex";
pub const DISPLAY_NAME: &str = "Codex";
pub const SUPPORTED_TYPES: &[CapabilityType] = &[
    CapabilityType::Skill,
    CapabilityType::Tool,
    CapabilityType::Hook,
    CapabilityType::Workflow,
    CapabilityType::McpServer,
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

    fn hook_settings_shape(&self) -> HookSettingsShape {
        HookSettingsShape::Grouped
    }

    fn detect(&self, repo_root: &Path) -> bool {
        repo_root.join(".agents").exists() || repo_root.join("AGENTS.md").exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC-106 D2: Codex shares Claude Code's remote-server shape, `type`
    /// and `${VAR}` both. Pinned per adapter rather than inferred from the
    /// shared default, because assuming harnesses agree is what produced
    /// debt item #1.
    #[test]
    fn a_remote_server_entry_declares_type_and_renders_headers() {
        let server = tuff_core::manifest::McpServerConfig {
            transport: tuff_core::manifest::McpTransport::Http,
            command: None,
            args: Vec::new(),
            url: Some("https://mcp.example.test/mcp".to_string()),
            env: Default::default(),
            headers: [(
                "Authorization".to_string(),
                tuff_core::manifest::HeaderRef {
                    from_env: "EXAMPLE_TOKEN".to_string(),
                    format: Some("Bearer {}".to_string()),
                },
            )]
            .into_iter()
            .collect(),
            metadata: None,
        };

        let entry = Codex.mcp_server_entry(&server);

        assert_eq!(
            entry,
            serde_json::json!({
                "type": "http",
                "url": "https://mcp.example.test/mcp",
                "headers": {"Authorization": "Bearer ${EXAMPLE_TOKEN}"},
            })
        );
    }

    #[test]
    fn id_and_display_name_are_not_empty() {
        assert!(!ID.is_empty());
        assert!(!DISPLAY_NAME.is_empty());
    }

    #[test]
    fn supported_types_covers_all_capability_types() {
        assert_eq!(SUPPORTED_TYPES.len(), 5);
    }

    #[test]
    fn merging_the_same_fragment_twice_does_not_duplicate_the_hook() {
        let fragment = serde_json::json!({
            "hooks": {
                "before_finish": [{"hooks": [{"type": "command", "command": "sh .agents/hooks/demo/run.sh"}]}]
            }
        });

        let once = Codex
            .merge_hook_fragment(None, &fragment)
            .expect("first merge");
        let twice = Codex
            .merge_hook_fragment(Some(&once), &fragment)
            .expect("second merge");

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
