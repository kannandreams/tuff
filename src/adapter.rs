use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::adapters::{codex, claude_code};
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
    pub kind: String,
    pub version: String,
    pub description: String,
    pub source_files: Vec<Vec<u8>>,
    pub source_dir: PathBuf,
}

pub fn resolve_primitive(manifest: &PrimitiveManifest) -> Result<ResolvedPrimitive> {
    let source_files = manifest.read_source_contents()?;
    Ok(ResolvedPrimitive {
        id: manifest.id.clone(),
        kind: manifest.kind.clone(),
        version: manifest.version.clone(),
        description: manifest.description.clone(),
        source_files,
        source_dir: manifest.root.clone(),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterKind {
    Codex,
    ClaudeCode,
}

impl AdapterKind {
    pub fn id(&self) -> &'static str {
        match self {
            Self::Codex => codex::ID,
            Self::ClaudeCode => claude_code::ID,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Codex => codex::DISPLAY_NAME,
            Self::ClaudeCode => claude_code::DISPLAY_NAME,
        }
    }

    pub fn all() -> Vec<Self> {
        vec![Self::Codex, Self::ClaudeCode]
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            codex::ID => Some(Self::Codex),
            claude_code::ID => Some(Self::ClaudeCode),
            _ => None,
        }
    }

    pub fn supports(&self, kind: &str) -> bool {
        match self {
            Self::Codex => codex::supports(kind),
            Self::ClaudeCode => claude_code::supports(kind),
        }
    }

    pub fn plan(
        &self,
        primitive: &ResolvedPrimitive,
        repo_root: &Path,
    ) -> Result<Vec<PlannedFile>> {
        match self {
            Self::Codex => codex::plan(primitive, repo_root),
            Self::ClaudeCode => claude_code::plan(primitive, repo_root),
        }
    }

    pub fn remove(&self, primitive_id: &str, repo_root: &Path) -> Result<()> {
        match self {
            Self::Codex => codex::remove(primitive_id, repo_root),
            Self::ClaudeCode => claude_code::remove(primitive_id, repo_root),
        }
    }

    #[allow(dead_code)]
    pub fn detect(&self, repo_root: &Path) -> bool {
        match self {
            Self::Codex => codex::detect(repo_root),
            Self::ClaudeCode => claude_code::detect(repo_root),
        }
    }

    pub fn kinds_supported(&self) -> Vec<&'static str> {
        match self {
            Self::Codex => codex::SUPPORTED_KINDS.to_vec(),
            Self::ClaudeCode => claude_code::SUPPORTED_KINDS.to_vec(),
        }
    }
}
