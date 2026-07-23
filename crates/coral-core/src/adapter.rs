use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use coral_hooks_spec::CompatibilityMatrix;

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
    pub allow_existing: bool,
}

impl PlannedFile {
    pub fn new(path: String, content: Vec<u8>) -> Self {
        Self {
            path,
            content,
            allow_existing: false,
        }
    }

    pub fn mergeable(path: String, content: Vec<u8>) -> Self {
        Self {
            path,
            content,
            allow_existing: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NativeHookConfig {
    pub fragment: serde_json::Value,
    pub source_files: Vec<(String, Vec<u8>)>,
}

#[derive(Debug, Clone)]
pub enum HookDefinition {
    Command(crate::manifest::HookConfig),
    Native(NativeHookConfig),
}

#[derive(Debug, Clone)]
pub enum CapabilityKind {
    Skill,
    Tool {
        parameters: serde_json::Value,
        implementation: crate::manifest::ImplementationConfig,
    },
    Hook {
        hook: HookDefinition,
    },
    Workflow {
        workflow: crate::manifest::WorkflowConfig,
    },
}

impl CapabilityKind {
    pub fn capability_type(&self) -> CapabilityType {
        match self {
            Self::Skill => CapabilityType::Skill,
            Self::Tool { .. } => CapabilityType::Tool,
            Self::Hook { .. } => CapabilityType::Hook,
            Self::Workflow { .. } => CapabilityType::Workflow,
        }
    }
}

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
    let kind =
        match manifest.capability_type {
            CapabilityType::Skill => CapabilityKind::Skill,
            CapabilityType::Tool => CapabilityKind::Tool {
                parameters: manifest.parameters.clone().ok_or_else(|| {
                    CoralError::new("tool capability requires [parameters] section")
                })?,
                implementation: manifest.implementation.clone().ok_or_else(|| {
                    CoralError::new("tool capability requires [implementation] section")
                })?,
            },
            CapabilityType::Hook => {
                CapabilityKind::Hook {
                    hook: HookDefinition::Command(manifest.hook.clone().ok_or_else(|| {
                        CoralError::new("hook capability requires [hook] section")
                    })?),
                }
            }
            CapabilityType::Workflow => CapabilityKind::Workflow {
                workflow: manifest.workflow.clone().ok_or_else(|| {
                    CoralError::new("workflow capability requires [workflow] section")
                })?,
            },
            CapabilityType::Policy => {
                return Err(CoralError::new(
                    "policy capabilities are not installable yet",
                ));
            }
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
    fn hook_compatibility(&self) -> &'static CompatibilityMatrix;
    fn hook_settings_relpath(&self) -> &'static str;
    fn scaffold_hook_event(&self) -> &'static str;
    fn hook_filename(&self) -> &'static str;
    fn hook_file_content(&self, hook_cfg: &crate::manifest::HookConfig) -> Result<Vec<u8>>;
    fn command_hook_fragment(&self, native_event: &str, command: &str) -> serde_json::Value;
    fn merge_hook_fragment(
        &self,
        existing: Option<&[u8]>,
        fragment: &serde_json::Value,
    ) -> Result<Vec<u8>>;
    fn remove_hook_settings(
        &self,
        repo_root: &Path,
        managed_hooks: &[crate::lockfile::ManagedHook],
    ) -> Result<()>;
    fn detect(&self, repo_root: &Path) -> bool;

    fn kinds_supported(&self) -> &[CapabilityType];

    fn supports(&self, capability_type: CapabilityType) -> bool {
        self.kinds_supported().contains(&capability_type)
    }

    fn native_hook_event(&self, raw_event: &str) -> Result<&'static str> {
        let matrix = self.hook_compatibility();
        let Some(entry) = matrix.find_event(raw_event) else {
            return Err(CoralError::new(format!(
                "{} does not support hook event '{}'. Supported events: {}",
                self.display_name(),
                raw_event,
                matrix.supported_native_events().join(", ")
            )));
        };
        entry.native_event_name().ok_or_else(|| {
            let suffix = entry
                .caveat
                .map(|caveat| format!(": {caveat}"))
                .unwrap_or_default();
            CoralError::new(format!(
                "{} does not support hook event '{}'{}",
                self.display_name(),
                raw_event,
                suffix
            ))
        })
    }

    fn ensure_project_dir(&self, repo_root: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(repo_root.join(self.dir_prefix()))
    }

    fn plan(&self, capability: &ResolvedCapability, repo_root: &Path) -> Result<Vec<PlannedFile>> {
        match capability.capability_type {
            CapabilityType::Tool => self.plan_tool(capability, repo_root),
            CapabilityType::Hook => self.plan_hook(capability, repo_root),
            CapabilityType::Workflow => self.plan_workflow(capability, repo_root),
            CapabilityType::Policy => Err(CoralError::new(
                "policy capabilities are not installable yet",
            )),
            _ => self.plan_skill(capability, repo_root),
        }
    }

    fn remove(
        &self,
        primitive_id: &str,
        repo_root: &Path,
        managed_hooks: &[crate::lockfile::ManagedHook],
    ) -> Result<()> {
        let prefix = self.dir_prefix();
        for kind in &["skills", "tools", "hooks", "workflows"] {
            self.remove_dir(repo_root, prefix, kind, primitive_id)?;
        }
        crate::mcp::remove_tool(&repo_root.join(self.mcp_config_relpath()), primitive_id)?;
        self.remove_hook_settings(repo_root, managed_hooks)?;
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

            files.push(PlannedFile::new(
                relative_or_absolute_fs(&target_path, repo_root),
                content.clone(),
            ));
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

            files.push(PlannedFile::new(
                relative_or_absolute_fs(&target_path, repo_root),
                content.clone(),
            ));
        }

        if capability.source_files.is_empty() {
            let placeholder = repo_root
                .join(self.dir_prefix())
                .join("tools")
                .join(&capability.id)
                .join(".gitkeep");
            files.push(PlannedFile::new(
                relative_or_absolute_fs(&placeholder, repo_root),
                vec![],
            ));
        }

        Ok(files)
    }

    fn plan_hook(
        &self,
        capability: &ResolvedCapability,
        repo_root: &Path,
    ) -> Result<Vec<PlannedFile>> {
        let CapabilityKind::Hook { hook } = &capability.kind else {
            return Err(CoralError::new("plan_hook called on non-hook capability"));
        };

        match hook {
            HookDefinition::Command(hook_cfg) => {
                let native_event = self.native_hook_event(&hook_cfg.event)?;
                let target_path = repo_root
                    .join(self.dir_prefix())
                    .join("hooks")
                    .join(&capability.id)
                    .join("run.sh");
                let script = self.hook_file_content(hook_cfg)?;
                let settings_relpath = self.hook_settings_relpath();
                let fragment = self.command_hook_fragment(
                    native_event,
                    &format!("sh {}/hooks/{}/run.sh", self.dir_prefix(), capability.id),
                );
                let settings_path = repo_root.join(settings_relpath);
                let existing = if settings_path.is_file() {
                    Some(std::fs::read(&settings_path)?)
                } else {
                    None
                };
                let merged = self.merge_hook_fragment(existing.as_deref(), &fragment)?;

                let mut files = vec![PlannedFile::new(
                    relative_or_absolute_fs(&target_path, repo_root),
                    script,
                )];
                for (relative, content) in &capability.source_files {
                    let path = repo_root
                        .join(self.dir_prefix())
                        .join("hooks")
                        .join(&capability.id)
                        .join(relative);
                    files.push(PlannedFile::new(
                        relative_or_absolute_fs(&path, repo_root),
                        content.clone(),
                    ));
                }
                files.push(PlannedFile::mergeable(
                    relative_or_absolute_fs(&settings_path, repo_root),
                    merged,
                ));
                Ok(files)
            }
            HookDefinition::Native(native) => self.plan_native_hook(capability, native, repo_root),
        }
    }

    fn plan_native_hook(
        &self,
        capability: &ResolvedCapability,
        native: &NativeHookConfig,
        repo_root: &Path,
    ) -> Result<Vec<PlannedFile>> {
        let hook_root = repo_root
            .join(self.dir_prefix())
            .join("hooks")
            .join(&capability.id);
        let hook_root_rel = relative_or_absolute_fs(&hook_root, repo_root);
        let in_harness_source =
            path_is_under(&capability.source_dir, &repo_root.join(self.dir_prefix()));

        let mut files = Vec::new();
        if in_harness_source {
            for (rel_path, content) in &native.source_files {
                let target_path = capability.source_dir.join(rel_path);
                files.push(PlannedFile::mergeable(
                    relative_or_absolute_fs(&target_path, repo_root),
                    content.clone(),
                ));
            }
        } else {
            for (rel_path, content) in &native.source_files {
                let target_path = hook_root.join(rel_path);
                files.push(PlannedFile::new(
                    relative_or_absolute_fs(&target_path, repo_root),
                    content.clone(),
                ));
            }
        }

        let fragment = replace_hook_dir_placeholder(native.fragment.clone(), &hook_root_rel);
        let settings_relpath = self.hook_settings_relpath();
        let settings_path = repo_root.join(settings_relpath);
        let existing = if settings_path.is_file() {
            Some(std::fs::read(&settings_path)?)
        } else {
            None
        };
        let merged = self.merge_hook_fragment(existing.as_deref(), &fragment)?;
        files.push(PlannedFile::mergeable(
            relative_or_absolute_fs(&settings_path, repo_root),
            merged,
        ));
        Ok(files)
    }

    fn plan_workflow(
        &self,
        capability: &ResolvedCapability,
        repo_root: &Path,
    ) -> Result<Vec<PlannedFile>> {
        let CapabilityKind::Workflow { workflow: wf } = &capability.kind else {
            return Err(CoralError::new(
                "plan_workflow called on non-workflow capability",
            ));
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

        Ok(vec![PlannedFile::new(
            relative_or_absolute_fs(&target_path, repo_root),
            content.into_bytes(),
        )])
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

fn path_is_under(path: &Path, root: &Path) -> bool {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical_path.starts_with(canonical_root)
}

pub fn replace_hook_dir_placeholder(
    mut value: serde_json::Value,
    hook_dir: &str,
) -> serde_json::Value {
    match &mut value {
        serde_json::Value::String(s) => {
            *s = s.replace("{{hook_dir}}", hook_dir);
        }
        serde_json::Value::Array(items) => {
            for item in items {
                *item = replace_hook_dir_placeholder(item.take(), hook_dir);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values_mut() {
                *item = replace_hook_dir_placeholder(item.take(), hook_dir);
            }
        }
        _ => {}
    }
    value
}

fn relative_or_absolute_fs(path: &Path, repo_root: &Path) -> String {
    crate::lockfile::relative_or_absolute_fs(path, repo_root)
}
