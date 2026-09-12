use std::path::Path;

pub use tuff_core::adapter::replace_hook_dir_placeholder;
pub use tuff_core::adapter::{
    AgentAdapter, CapabilityKind, EmittedFile, HookDefinition, HookRenderContext,
    HookSettingsShape, NativeHookConfig, PlannedFile, ResolvedCapability, resolve_capability,
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

/// `AdapterKind` is an adapter by forwarding every declared method to the
/// harness's singleton. Only the declarations are listed: the trait's
/// default methods (planning, hook rendering, removal) run on `AdapterKind`
/// itself and reach the singleton through these.
macro_rules! forward_to_implementation {
    ($( fn $name:ident(&self $(, $arg:ident : $ty:ty)* ) $(-> $ret:ty)? ; )*) => {
        impl AgentAdapter for AdapterKind {
            $(
                fn $name(&self $(, $arg: $ty)*) $(-> $ret)? {
                    self.implementation().$name($($arg),*)
                }
            )*
        }
    };
}

forward_to_implementation! {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn dir_prefix(&self) -> &'static str;
    fn mcp_config_relpath(&self) -> &'static str;
    fn mcp_env_reference(&self, var: &str) -> String;
    fn mcp_server_entry(&self, server: &tuff_core::manifest::McpServerConfig) -> serde_json::Value;
    fn mcp_http_declares_type(&self) -> bool;
    fn supported_agents(&self) -> &[&'static str];
    fn hook_compatibility(&self) -> &'static CompatibilityMatrix;
    fn hook_settings_relpath(&self) -> &'static str;
    fn hook_settings_shape(&self) -> HookSettingsShape;
    fn scaffold_hook_event(&self) -> &'static str;
    fn hook_filename(&self) -> &'static str;
    fn hook_file_content(&self, hook_cfg: &HookConfig) -> Result<Vec<u8>>;
    fn command_hook_fragment(&self, native_event: &str, command: &str) -> serde_json::Value;
    fn merge_hook_fragment(&self, existing: Option<&[u8]>, fragment: &serde_json::Value) -> Result<Vec<u8>>;
    fn remove_hook_settings(&self, repo_root: &Path, managed_hooks: &[tuff_core::lockfile::ManagedHook]) -> Result<()>;
    fn detect(&self, repo_root: &Path) -> bool;
    fn kinds_supported(&self) -> &[CapabilityType];
}
