use std::path::Path;

use tuff_hooks_spec::{
    CompatibilityEntry, CompatibilityMatrix, CoverageLevel, HookEvent, SPEC_VERSION,
};

use tuff_core::adapter::{AgentAdapter, HookSettingsShape};
use tuff_core::manifest::CapabilityType;

pub const ID: &str = "pi";
pub const DISPLAY_NAME: &str = "Pi";

/// MCP servers, which Pi reads from `.pi/mcp.json`. Skills reach Pi through
/// the `open-agents` layout, since Pi reads `.agents/skills/`. Pi has no
/// permission rules, and its hooks are TypeScript extensions, which this
/// adapter does not manage.
pub const SUPPORTED_TYPES: &[CapabilityType] = &[CapabilityType::McpServer];

pub const SUPPORTED_AGENTS: &[&str] = &["Pi"];

/// Pi's project MCP config, checked against Pi 1.1.0 on 2026-10-10. Its
/// `mcpServers` entries take `command`, `args`, and `env`, or `url` and
/// `headers`, and expand `${VAR}` in `env` and `headers` values.
pub const MCP_CONFIG_RELPATH: &str = ".pi/mcp.json";

/// Pi's project settings file, where extensions are listed. No hook is
/// written to it, since this adapter takes no hooks.
pub const SETTINGS_RELPATH: &str = ".pi/settings.json";

pub struct Pi;

const fn unsupported_hook(event: HookEvent) -> CompatibilityEntry {
    CompatibilityEntry {
        event,
        native_event: None,
        aliases: &[],
        coverage: CoverageLevel::Unsupported,
        scope: &[],
        caveat: Some("Tuff does not manage Pi hooks, which Pi loads as TypeScript extensions."),
        source: None,
        since_harness_version: None,
        until_harness_version: None,
    }
}

/// Every event is unsupported: this adapter takes no hooks, and
/// `tuff hooks spec` lists only adapters that do.
pub const HOOK_COMPATIBILITY: CompatibilityMatrix = CompatibilityMatrix {
    spec_version: SPEC_VERSION,
    adapter: ID,
    events: &[
        unsupported_hook(HookEvent::SessionStart),
        unsupported_hook(HookEvent::SessionEnd),
        unsupported_hook(HookEvent::PreToolUse),
        unsupported_hook(HookEvent::PostToolUse),
        unsupported_hook(HookEvent::BeforeFinish),
        unsupported_hook(HookEvent::AfterSave),
        unsupported_hook(HookEvent::Stop),
    ],
};

impl AgentAdapter for Pi {
    fn id(&self) -> &'static str {
        ID
    }

    fn display_name(&self) -> &'static str {
        DISPLAY_NAME
    }

    fn dir_prefix(&self) -> &'static str {
        ".pi"
    }

    fn mcp_config_relpath(&self) -> &'static str {
        MCP_CONFIG_RELPATH
    }

    /// Pi picks the transport from `command` or `url`, and its documented
    /// entries leave `type` out.
    fn mcp_http_declares_type(&self) -> bool {
        false
    }

    /// Pi reads `.pi/mcp.json` only once the project is trusted.
    fn install_note(&self, kind: CapabilityType) -> Option<&'static str> {
        (kind == CapabilityType::McpServer).then_some(
            "Pi reads .pi/mcp.json only in a trusted project; approve the project when Pi asks, or run /trust",
        )
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
        ""
    }

    fn hook_settings_shape(&self) -> HookSettingsShape {
        HookSettingsShape::Flat
    }

    fn detect(&self, repo_root: &Path) -> bool {
        repo_root.join(".pi").exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuff_core::manifest::{EnvRef, HeaderRef, McpServerConfig, McpTransport};

    #[test]
    fn mcp_entries_take_pis_shape_and_env_syntax() {
        let local = McpServerConfig {
            transport: McpTransport::Stdio,
            command: Some("npx".to_string()),
            args: vec!["-y".to_string(), "pkg".to_string()],
            url: None,
            env: [(
                "GITHUB_PERSONAL_ACCESS_TOKEN".to_string(),
                EnvRef {
                    from_env: "GITHUB_PERSONAL_ACCESS_TOKEN".to_string(),
                },
            )]
            .into_iter()
            .collect(),
            headers: Default::default(),
            metadata: None,
        };
        assert_eq!(
            Pi.mcp_server_entry(&local),
            serde_json::json!({
                "command": "npx",
                "args": ["-y", "pkg"],
                "env": {"GITHUB_PERSONAL_ACCESS_TOKEN": "${GITHUB_PERSONAL_ACCESS_TOKEN}"},
            })
        );

        let remote = McpServerConfig {
            transport: McpTransport::Http,
            command: None,
            args: Vec::new(),
            url: Some("https://mcp.example.test/mcp".to_string()),
            env: Default::default(),
            headers: [(
                "Authorization".to_string(),
                HeaderRef {
                    from_env: "EXAMPLE_TOKEN".to_string(),
                    format: Some("Bearer {}".to_string()),
                },
            )]
            .into_iter()
            .collect(),
            metadata: None,
        };
        assert_eq!(
            Pi.mcp_server_entry(&remote),
            serde_json::json!({
                "url": "https://mcp.example.test/mcp",
                "headers": {"Authorization": "Bearer ${EXAMPLE_TOKEN}"},
            })
        );
    }

    #[test]
    fn pi_takes_mcp_servers_only() {
        assert_eq!(SUPPORTED_TYPES, [CapabilityType::McpServer]);
        assert!(
            HOOK_COMPATIBILITY
                .events
                .iter()
                .all(|entry| entry.coverage == CoverageLevel::Unsupported)
        );
        assert!(
            Pi.policy_compatibility()
                .iter()
                .all(|entry| entry.coverage == CoverageLevel::Unsupported)
        );
    }
}
