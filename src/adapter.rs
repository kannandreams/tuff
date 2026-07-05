use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::adapters::{claude, open_agents};
use crate::error::Result;
use crate::manifest::PrimitiveManifest;

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
pub struct ResolvedPrimitive {
    pub id: String,
    pub primitive: String,
    pub version: String,
    pub description: String,
    pub source_files: Vec<(String, Vec<u8>)>,
    pub source_dir: PathBuf,
}

pub fn resolve_primitive(manifest: &PrimitiveManifest) -> Result<ResolvedPrimitive> {
    let source_files = manifest.read_source_contents_with_names()?;
    Ok(ResolvedPrimitive {
        id: manifest.id.clone(),
        primitive: manifest.primitive.clone(),
        version: manifest.version.clone(),
        description: manifest.description.clone(),
        source_files,
        source_dir: manifest.root.clone(),
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

    pub fn supports(&self, primitive: &str) -> bool {
        match self {
            Self::OpenAgents => open_agents::supports(primitive),
            Self::Claude => claude::supports(primitive),
        }
    }

    pub fn plan(
        &self,
        primitive: &ResolvedPrimitive,
        repo_root: &Path,
    ) -> Result<Vec<PlannedFile>> {
        match self {
            Self::OpenAgents => open_agents::plan(primitive, repo_root),
            Self::Claude => claude::plan(primitive, repo_root),
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
            Self::OpenAgents => open_agents::SUPPORTED_KINDS,
            Self::Claude => claude::SUPPORTED_KINDS,
        }
    }

    pub fn supported_agents(&self) -> &[&'static str] {
        match self {
            Self::OpenAgents => open_agents::SUPPORTED_AGENTS,
            Self::Claude => claude::SUPPORTED_AGENTS,
        }
    }
}
