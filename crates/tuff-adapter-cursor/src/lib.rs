use std::path::Path;

use tuff_hooks_spec::{
    CompatibilityEntry, CompatibilityMatrix, CoverageLevel, HookEvent, SPEC_VERSION,
};

use tuff_core::adapter::{AgentAdapter, HookSettingsShape};
use tuff_core::manifest::CapabilityType;

pub const ID: &str = "cursor";
pub const DISPLAY_NAME: &str = "Cursor";
pub const SUPPORTED_TYPES: &[CapabilityType] = &[
    CapabilityType::Skill,
    CapabilityType::Tool,
    CapabilityType::Hook,
    CapabilityType::Workflow,
    CapabilityType::McpServer,
];

pub const SUPPORTED_AGENTS: &[&str] = &["Cursor"];

pub const HOOK_SETTINGS_RELPATH: &str = ".cursor/hooks.json";

pub struct Cursor;

pub const HOOK_COMPATIBILITY: CompatibilityMatrix = CompatibilityMatrix {
    spec_version: SPEC_VERSION,
    adapter: ID,
    events: &[
        CompatibilityEntry {
            event: HookEvent::SessionStart,
            native_event: Some("sessionStart"),
            aliases: &[],
            coverage: CoverageLevel::Full,
            scope: &[],
            caveat: None,
            source: Some("https://cursor.com/blog/agent-best-practices"),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::SessionEnd,
            native_event: Some("sessionEnd"),
            aliases: &[],
            coverage: CoverageLevel::Full,
            scope: &["session lifecycle"],
            caveat: None,
            source: Some("https://cursor.com/blog/agent-best-practices"),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::PreToolUse,
            native_event: Some("preToolUse"),
            aliases: &[],
            coverage: CoverageLevel::Full,
            scope: &["agent tool calls"],
            caveat: Some("A native matcher can narrow the tools that receive the hook."),
            source: Some("https://cursor.com/blog/agent-best-practices"),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::PostToolUse,
            native_event: Some("postToolUse"),
            aliases: &[],
            coverage: CoverageLevel::Full,
            scope: &["agent tool calls"],
            caveat: Some("A native matcher can narrow the tools that receive the hook."),
            source: Some("https://cursor.com/blog/agent-best-practices"),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::AfterSave,
            native_event: None,
            aliases: &[],
            coverage: CoverageLevel::Unsupported,
            scope: &[],
            caveat: Some("Cursor does not expose a direct after-save hook in this adapter."),
            source: None,
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::BeforeFinish,
            native_event: Some("stop"),
            aliases: &[],
            coverage: CoverageLevel::Partial,
            scope: &["agent completion"],
            caveat: Some("Cursor stop can request continuation rather than block a prior action."),
            source: Some("https://cursor.com/blog/agent-best-practices"),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::Stop,
            native_event: Some("stop"),
            aliases: &[],
            coverage: CoverageLevel::Full,
            scope: &["agent completion"],
            caveat: None,
            source: Some("https://cursor.com/blog/agent-best-practices"),
            since_harness_version: None,
            until_harness_version: None,
        },
    ],
};

impl AgentAdapter for Cursor {
    fn id(&self) -> &'static str {
        ID
    }

    fn display_name(&self) -> &'static str {
        DISPLAY_NAME
    }

    fn dir_prefix(&self) -> &'static str {
        ".cursor"
    }

    fn mcp_config_relpath(&self) -> &'static str {
        ".cursor/mcp.json"
    }

    /// Cursor interpolates `${env:VAR}` in `mcp.json`, not the bare `${VAR}`
    /// form Claude Code and most stdio clients use.
    fn mcp_env_reference(&self, var: &str) -> String {
        format!("${{env:{var}}}")
    }

    /// Cursor's remote-server entry is `{ "url": …, "headers": … }` with no
    /// `type` key; it distinguishes stdio from remote by the presence of
    /// `command` versus `url`.
    fn mcp_http_declares_type(&self) -> bool {
        false
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
        "sessionStart"
    }

    fn hook_settings_shape(&self) -> HookSettingsShape {
        HookSettingsShape::Flat
    }

    fn detect(&self, repo_root: &Path) -> bool {
        repo_root.join(".cursor").exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC-106 D2: Cursor's remote entry has no `type` key and spells a
    /// variable `${env:VAR}`, unlike every other harness Tuff targets.
    #[test]
    fn a_remote_server_entry_omits_type_and_uses_cursor_variable_syntax() {
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

        let entry = Cursor.mcp_server_entry(&server);

        assert_eq!(
            entry,
            serde_json::json!({
                "url": "https://mcp.example.test/mcp",
                "headers": {"Authorization": "Bearer ${env:EXAMPLE_TOKEN}"},
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
                "beforeSubmitPrompt": [{"hooks": [{"type": "command", "command": "sh .cursor/hooks/demo/run.sh"}]}]
            }
        });

        let once = Cursor
            .merge_hook_fragment(None, &fragment)
            .expect("first merge");
        let twice = Cursor
            .merge_hook_fragment(Some(&once), &fragment)
            .expect("second merge");

        let settings: serde_json::Value = serde_json::from_slice(&twice).expect("valid json");
        let groups = settings["hooks"]["beforeSubmitPrompt"]
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
