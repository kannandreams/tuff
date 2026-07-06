use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::adapters::{claude, open_agents};
use crate::error::Result;
use crate::manifest::CapabilityManifest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmittedFile {
    pub path: String,
    pub hash: String,
}

#[derive(Debug, Clone)]
pub struct PlannedFile {
    pub path: String,
    pub content: Vec<u8>,
}

#[allow(dead_code)]
pub struct ResolvedCapability {
    pub id: String,
    pub capability_type: String,
    pub version: String,
    pub description: String,
    pub source_files: Vec<(String, Vec<u8>)>,
    pub source_dir: PathBuf,
    pub parameters: Option<serde_json::Value>,
    pub implementation: Option<crate::manifest::ImplementationConfig>,
    pub hook: Option<crate::manifest::HookConfig>,
    pub workflow: Option<crate::manifest::WorkflowConfig>,
}

pub fn resolve_capability(manifest: &CapabilityManifest) -> Result<ResolvedCapability> {
    let source_files = manifest.read_source_contents_with_names()?;
    Ok(ResolvedCapability {
        id: manifest.id.clone(),
        capability_type: manifest.capability_type.clone(),
        version: manifest.version.clone(),
        description: manifest.description.clone(),
        source_files,
        source_dir: manifest.root.clone(),
        parameters: manifest.parameters.clone(),
        implementation: manifest.implementation.clone(),
        hook: manifest.hook.clone(),
        workflow: manifest.workflow.clone(),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterKind {
    OpenAgents,
    Claude,
}

impl AdapterKind {
    pub fn id(&self) -> &'static str {
        match self {
            Self::OpenAgents => open_agents::ID,
            Self::Claude => claude::ID,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::OpenAgents => open_agents::DISPLAY_NAME,
            Self::Claude => claude::DISPLAY_NAME,
        }
    }

    pub fn all() -> Vec<Self> {
        vec![Self::OpenAgents, Self::Claude]
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            open_agents::ID => Some(Self::OpenAgents),
            "codex" => Some(Self::OpenAgents),
            claude::ID => Some(Self::Claude),
            "claude-code" => Some(Self::Claude),
            _ => None,
        }
    }

    pub fn supports(&self, capability_type: &str) -> bool {
        match self {
            Self::OpenAgents => open_agents::supports(capability_type),
            Self::Claude => claude::supports(capability_type),
        }
    }

    pub fn plan(
        &self,
        capability: &ResolvedCapability,
        repo_root: &Path,
    ) -> Result<Vec<PlannedFile>> {
        match self {
            Self::OpenAgents => open_agents::plan(capability, repo_root),
            Self::Claude => claude::plan(capability, repo_root),
        }
    }

    pub fn remove(&self, primitive_id: &str, repo_root: &Path) -> Result<()> {
        match self {
            Self::OpenAgents => open_agents::remove(primitive_id, repo_root),
            Self::Claude => claude::remove(primitive_id, repo_root),
        }
    }

    #[allow(dead_code)]
    pub fn detect(&self, repo_root: &Path) -> bool {
        match self {
            Self::OpenAgents => open_agents::detect(repo_root),
            Self::Claude => claude::detect(repo_root),
        }
    }

    pub fn kinds_supported(&self) -> &[&'static str] {
        match self {
            Self::OpenAgents => open_agents::SUPPORTED_TYPES,
            Self::Claude => claude::SUPPORTED_TYPES,
        }
    }

    pub fn supported_agents(&self) -> &[&'static str] {
        match self {
            Self::OpenAgents => open_agents::SUPPORTED_AGENTS,
            Self::Claude => claude::SUPPORTED_AGENTS,
        }
    }

    pub fn supported_events(&self) -> &[&'static str] {
        match self {
            Self::OpenAgents => open_agents::SUPPORTED_EVENTS,
            Self::Claude => claude::SUPPORTED_EVENTS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_id_returns_open_agents_for_codex_alias() {
        assert_eq!(AdapterKind::from_id("codex"), Some(AdapterKind::OpenAgents));
        assert_eq!(AdapterKind::from_id("open-agents"), Some(AdapterKind::OpenAgents));
    }

    #[test]
    fn from_id_returns_claude_for_claude_code_alias() {
        assert_eq!(AdapterKind::from_id("claude-code"), Some(AdapterKind::Claude));
        assert_eq!(AdapterKind::from_id("claude"), Some(AdapterKind::Claude));
    }

    #[test]
    fn from_id_returns_none_for_unknown() {
        assert_eq!(AdapterKind::from_id("nonexistent"), None);
    }

    #[test]
    fn all_returns_two_adapters() {
        let all = AdapterKind::all();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn display_name_not_empty() {
        for a in AdapterKind::all() {
            assert!(!a.display_name().is_empty());
            assert!(!a.id().is_empty());
        }
    }
}
