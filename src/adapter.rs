use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::adapters::{claude, open_agents};
use crate::error::{CoralError, Result};
use crate::manifest::{CapabilityManifest, CapabilityType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmittedFile {
    pub path: String,
    pub hash: String,
    #[serde(rename = "baselineHash")]
    pub baseline_hash: String,
}

#[derive(Debug, Clone)]
pub struct PlannedFile {
    pub path: String,
    pub content: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum CapabilityKind {
    Skill,
    Tool {
        #[expect(dead_code, reason = "parameters JSON Schema is consumed by the agent at runtime, not by Coral itself")]
        parameters: serde_json::Value,
        implementation: crate::manifest::ImplementationConfig,
    },
    Hook {
        hook: crate::manifest::HookConfig,
    },
    Workflow {
        workflow: crate::manifest::WorkflowConfig,
    },
}

impl CapabilityKind {
    #[expect(dead_code, reason = "utility method for code that only has a CapabilityKind value")]
    pub fn capability_type(&self) -> CapabilityType {
        match self {
            Self::Skill => CapabilityType::Skill,
            Self::Tool { .. } => CapabilityType::Tool,
            Self::Hook { .. } => CapabilityType::Hook,
            Self::Workflow { .. } => CapabilityType::Workflow,
        }
    }
}

#[expect(dead_code, reason = "ResolvedCapability is used throughout the codebase; dead_code is a false positive")]
pub struct ResolvedCapability {
    pub id: String,
    pub capability_type: CapabilityType,
    pub version: String,
    pub description: String,
    pub source_files: Vec<(String, Vec<u8>)>,
    pub source_dir: PathBuf,
    pub kind: CapabilityKind,
}

pub fn resolve_capability(manifest: &CapabilityManifest) -> Result<ResolvedCapability> {
    let source_files = manifest.read_source_contents_with_names()?;
    let kind = match manifest.capability_type {
        CapabilityType::Skill => CapabilityKind::Skill,
        CapabilityType::Tool => CapabilityKind::Tool {
            parameters: manifest.parameters.clone().ok_or_else(|| {
                CoralError::new("tool capability requires [parameters] section")
            })?,
            implementation: manifest.implementation.clone().ok_or_else(|| {
                CoralError::new("tool capability requires [implementation] section")
            })?,
        },
        CapabilityType::Hook => CapabilityKind::Hook {
            hook: manifest.hook.clone().ok_or_else(|| {
                CoralError::new("hook capability requires [hook] section")
            })?,
        },
        CapabilityType::Workflow => CapabilityKind::Workflow {
            workflow: manifest.workflow.clone().ok_or_else(|| {
                CoralError::new("workflow capability requires [workflow] section")
            })?,
        },
    };
    Ok(ResolvedCapability {
        id: manifest.id.clone(),
        capability_type: manifest.capability_type,
        version: manifest.version.clone(),
        description: manifest.description.clone(),
        source_files,
        source_dir: manifest.root.clone(),
        kind,
    })
}

pub trait AgentAdapter {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn dir_prefix(&self) -> &'static str;
    fn mcp_config_relpath(&self) -> &'static str;
    fn supported_agents(&self) -> &[&'static str];
    fn supported_events(&self) -> &[&'static str];
    fn hook_filename(&self) -> &'static str;
    fn hook_file_content(
        &self,
        hook_cfg: &crate::manifest::HookConfig,
    ) -> Result<Vec<u8>>;
    #[expect(dead_code, reason = "part of public adapter API; may be used by external callers")]
    fn detect(&self, repo_root: &Path) -> bool;

    fn kinds_supported(&self) -> &[CapabilityType];

    fn supports(&self, capability_type: CapabilityType) -> bool {
        self.kinds_supported().contains(&capability_type)
    }

    fn ensure_project_dir(&self, repo_root: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(repo_root.join(self.dir_prefix()))
    }

    fn plan(
        &self,
        capability: &ResolvedCapability,
        repo_root: &Path,
    ) -> Result<Vec<PlannedFile>> {
        match capability.capability_type {
            CapabilityType::Tool => self.plan_tool(capability, repo_root),
            CapabilityType::Hook => self.plan_hook(capability, repo_root),
            CapabilityType::Workflow => self.plan_workflow(capability, repo_root),
            _ => self.plan_skill(capability, repo_root),
        }
    }

    fn remove(&self, primitive_id: &str, repo_root: &Path) -> Result<()> {
        let prefix = self.dir_prefix();
        for kind in &["skills", "tools", "hooks", "workflows"] {
            self.remove_dir(repo_root, prefix, kind, primitive_id)?;
        }
        crate::adapters::mcp_remove_tool(
            repo_root,
            &repo_root.join(self.mcp_config_relpath()),
            primitive_id,
        )?;
        Ok(())
    }

    // ── internal helpers ───────────────────────────────────────────────

    fn plan_skill(
        &self,
        capability: &ResolvedCapability,
        repo_root: &Path,
    ) -> Result<Vec<PlannedFile>> {
        if capability.source_files.is_empty() {
            return Err(CoralError::new("no source files to emit"));
        }

        let mut files = Vec::new();
        for (rel_path, content) in &capability.source_files {
            let target_path = repo_root
                .join(self.dir_prefix())
                .join("skills")
                .join(&capability.id)
                .join(rel_path);

            files.push(PlannedFile {
                path: relative_or_absolute_fs(&target_path, repo_root),
                content: content.clone(),
            });
        }
        Ok(files)
    }

    fn plan_tool(
        &self,
        capability: &ResolvedCapability,
        repo_root: &Path,
    ) -> Result<Vec<PlannedFile>> {
        let mut files = Vec::new();

        for (rel_path, content) in &capability.source_files {
            let target_path = repo_root
                .join(self.dir_prefix())
                .join("tools")
                .join(&capability.id)
                .join(rel_path);

            files.push(PlannedFile {
                path: relative_or_absolute_fs(&target_path, repo_root),
                content: content.clone(),
            });
        }

        if capability.source_files.is_empty() {
            let placeholder = repo_root
                .join(self.dir_prefix())
                .join("tools")
                .join(&capability.id)
                .join(".gitkeep");
            files.push(PlannedFile {
                path: relative_or_absolute_fs(&placeholder, repo_root),
                content: vec![],
            });
        }

        Ok(files)
    }

    fn plan_hook(
        &self,
        capability: &ResolvedCapability,
        repo_root: &Path,
    ) -> Result<Vec<PlannedFile>> {
        let CapabilityKind::Hook { hook: hook_cfg } = &capability.kind else {
            return Err(CoralError::new("plan_hook called on non-hook capability"));
        };

        let target_path = repo_root
            .join(self.dir_prefix())
            .join("hooks")
            .join(&capability.id)
            .join(self.hook_filename());

        Ok(vec![PlannedFile {
            path: relative_or_absolute_fs(&target_path, repo_root),
            content: self.hook_file_content(hook_cfg)?,
        }])
    }

    fn plan_workflow(
        &self,
        capability: &ResolvedCapability,
        repo_root: &Path,
    ) -> Result<Vec<PlannedFile>> {
        let CapabilityKind::Workflow { workflow: wf } = &capability.kind else {
            return Err(CoralError::new("plan_workflow called on non-workflow capability"));
        };

        let target_path = repo_root
            .join(self.dir_prefix())
            .join("workflows")
            .join(&capability.id)
            .join("workflow.toml");

        let mut content = format!(
            "id = \"{}\"\nversion = \"{}\"\ntype = \"workflow\"\ndescription = \"{}\"\n",
            capability.id, capability.version, capability.description
        );
        for req in &wf.requires {
            content.push_str(&format!(
                "[[workflow.requires]]\nid = \"{}\"\ntype = \"{}\"\n",
                req.id, req.capability_type
            ));
        }

        Ok(vec![PlannedFile {
            path: relative_or_absolute_fs(&target_path, repo_root),
            content: content.into_bytes(),
        }])
    }

    fn remove_dir(
        &self,
        repo_root: &Path,
        base: &str,
        kind: &str,
        primitive_id: &str,
    ) -> Result<()> {
        let dir = repo_root.join(base).join(kind).join(primitive_id);

        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }

        let kind_dir = dir.parent().expect("kind dir should have parent");
        if kind_dir.exists() {
            let mut rd = match std::fs::read_dir(kind_dir) {
                Ok(rd) => rd,
                Err(_) => return Ok(()),
            };
            if rd.next().is_none() {
                std::fs::remove_dir(kind_dir)?;
            }
        }

        let base_dir = kind_dir.parent().expect("base dir should have parent");
        if base_dir.exists() {
            let mut rd = match std::fs::read_dir(base_dir) {
                Ok(rd) => rd,
                Err(_) => return Ok(()),
            };
            if rd.next().is_none() {
                std::fs::remove_dir(base_dir)?;
            }
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterKind {
    OpenAgents,
    Claude,
}

impl AgentAdapter for AdapterKind {
    fn id(&self) -> &'static str {
        match self {
            Self::OpenAgents => open_agents::ID,
            Self::Claude => claude::ID,
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            Self::OpenAgents => open_agents::DISPLAY_NAME,
            Self::Claude => claude::DISPLAY_NAME,
        }
    }

    fn dir_prefix(&self) -> &'static str {
        match self {
            Self::OpenAgents => ".agents",
            Self::Claude => ".claude",
        }
    }

    fn mcp_config_relpath(&self) -> &'static str {
        match self {
            Self::OpenAgents => ".agents/mcp.json",
            Self::Claude => ".mcp.json",
        }
    }

    fn supported_agents(&self) -> &[&'static str] {
        match self {
            Self::OpenAgents => open_agents::SUPPORTED_AGENTS,
            Self::Claude => claude::SUPPORTED_AGENTS,
        }
    }

    fn kinds_supported(&self) -> &[CapabilityType] {
        match self {
            Self::OpenAgents => open_agents::SUPPORTED_TYPES,
            Self::Claude => claude::SUPPORTED_TYPES,
        }
    }

    fn supported_events(&self) -> &[&'static str] {
        match self {
            Self::OpenAgents => open_agents::SUPPORTED_EVENTS,
            Self::Claude => claude::SUPPORTED_EVENTS,
        }
    }

    fn hook_filename(&self) -> &'static str {
        match self {
            Self::OpenAgents => "hook.toml",
            Self::Claude => "hook.json",
        }
    }

    fn hook_file_content(
        &self,
        hook_cfg: &crate::manifest::HookConfig,
    ) -> Result<Vec<u8>> {
        match self {
            Self::OpenAgents => Ok(format!(
                "event = \"{}\"\ncommand = \"{}\"\nworking_directory = \"{}\"\n",
                hook_cfg.event, hook_cfg.command, hook_cfg.working_directory
            )
            .into_bytes()),
            Self::Claude => {
                let json = serde_json::json!({
                    "event": hook_cfg.event,
                    "command": hook_cfg.command,
                    "working_directory": hook_cfg.working_directory,
                });
                Ok(serde_json::to_string_pretty(&json)?.into_bytes())
            }
        }
    }

    fn detect(&self, repo_root: &Path) -> bool {
        match self {
            Self::OpenAgents => open_agents::detect(repo_root),
            Self::Claude => claude::detect(repo_root),
        }
    }
}

impl AdapterKind {
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
}

fn relative_or_absolute_fs(path: &Path, repo_root: &Path) -> String {
    crate::lockfile::relative_or_absolute_fs(path, repo_root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_id_returns_open_agents_for_codex_alias() {
        assert_eq!(AdapterKind::from_id("codex"), Some(AdapterKind::OpenAgents));
        assert_eq!(
            AdapterKind::from_id("open-agents"),
            Some(AdapterKind::OpenAgents)
        );
    }

    #[test]
    fn from_id_returns_claude_for_claude_code_alias() {
        assert_eq!(
            AdapterKind::from_id("claude-code"),
            Some(AdapterKind::Claude)
        );
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
            assert!(!a.id().is_empty());
            assert!(!a.display_name().is_empty());
        }
    }

    #[test]
    fn trait_plan_delegates_to_variant() {
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let cap = ResolvedCapability {
            id: "test".into(),
            capability_type: CapabilityType::Skill,
            version: "1".into(),
            description: "d".into(),
            source_files: vec![("SKILL.md".into(), b"hello".to_vec())],
            source_dir: tmp.path().to_path_buf(),
            kind: CapabilityKind::Skill,
        };

        for a in AdapterKind::all() {
            let planned = a.plan(&cap, tmp.path()).unwrap();
            assert_eq!(planned.len(), 1);
            assert!(planned[0].path.contains("SKILL.md"));
        }
    }

    #[test]
    fn trait_remove_cleans_directories() {
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        for a in AdapterKind::all() {
            let dir = tmp.path().join(a.dir_prefix()).join("tools").join("test");
            std::fs::create_dir_all(&dir).unwrap();
            assert!(a.remove("test", tmp.path()).is_ok());
            assert!(!dir.exists());
        }
    }
}
