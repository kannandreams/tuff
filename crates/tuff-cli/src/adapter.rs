use std::path::Path;

pub use tuff_core::adapter::replace_hook_dir_placeholder;
pub use tuff_core::adapter::{
    AgentAdapter, CapabilityKind, EmittedFile, HookDefinition, HookRenderContext, NativeHookConfig,
    PlannedFile, ResolvedCapability, resolve_capability,
};
use tuff_core::error::Result;
use tuff_core::manifest::{CapabilityType, HookConfig};
use tuff_hooks_spec::CompatibilityMatrix;

use tuff_adapter_claude::Claude;
use tuff_adapter_codex::Codex;
use tuff_adapter_cursor::Cursor;
use tuff_adapter_open_agents::OpenAgents;

static OPEN_AGENTS: OpenAgents = OpenAgents;
static CLAUDE: Claude = Claude;
static CODEX: Codex = Codex;
static CURSOR: Cursor = Cursor;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterKind {
    OpenAgents,
    Claude,
    Codex,
    Cursor,
}

impl AdapterKind {
    pub fn all() -> Vec<Self> {
        vec![Self::OpenAgents, Self::Claude, Self::Codex, Self::Cursor]
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "open-agents" => Some(Self::OpenAgents),
            "claude" | "claude-code" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "cursor" => Some(Self::Cursor),
            _ => None,
        }
    }

    fn implementation(self) -> &'static dyn AgentAdapter {
        match self {
            Self::OpenAgents => &OPEN_AGENTS,
            Self::Claude => &CLAUDE,
            Self::Codex => &CODEX,
            Self::Cursor => &CURSOR,
        }
    }
}

impl AgentAdapter for AdapterKind {
    fn id(&self) -> &'static str {
        self.implementation().id()
    }

    fn display_name(&self) -> &'static str {
        self.implementation().display_name()
    }

    fn dir_prefix(&self) -> &'static str {
        self.implementation().dir_prefix()
    }

    fn mcp_config_relpath(&self) -> &'static str {
        self.implementation().mcp_config_relpath()
    }

    fn mcp_env_reference(&self, var: &str) -> String {
        self.implementation().mcp_env_reference(var)
    }

    fn mcp_server_entry(&self, server: &tuff_core::manifest::McpServerConfig) -> serde_json::Value {
        self.implementation().mcp_server_entry(server)
    }

    fn supported_agents(&self) -> &[&'static str] {
        self.implementation().supported_agents()
    }

    fn hook_compatibility(&self) -> &'static CompatibilityMatrix {
        self.implementation().hook_compatibility()
    }

    fn hook_settings_relpath(&self) -> &'static str {
        self.implementation().hook_settings_relpath()
    }

    fn scaffold_hook_event(&self) -> &'static str {
        self.implementation().scaffold_hook_event()
    }

    fn hook_filename(&self) -> &'static str {
        self.implementation().hook_filename()
    }

    fn hook_file_content(&self, hook_cfg: &HookConfig) -> Result<Vec<u8>> {
        self.implementation().hook_file_content(hook_cfg)
    }

    fn command_hook_fragment(&self, native_event: &str, command: &str) -> serde_json::Value {
        self.implementation()
            .command_hook_fragment(native_event, command)
    }

    fn merge_hook_fragment(
        &self,
        existing: Option<&[u8]>,
        fragment: &serde_json::Value,
    ) -> Result<Vec<u8>> {
        self.implementation()
            .merge_hook_fragment(existing, fragment)
    }

    fn remove_hook_settings(
        &self,
        repo_root: &Path,
        managed_hooks: &[tuff_core::lockfile::ManagedHook],
    ) -> Result<()> {
        self.implementation()
            .remove_hook_settings(repo_root, managed_hooks)
    }

    fn detect(&self, repo_root: &Path) -> bool {
        self.implementation().detect(repo_root)
    }

    fn kinds_supported(&self) -> &[CapabilityType] {
        self.implementation().kinds_supported()
    }
}
