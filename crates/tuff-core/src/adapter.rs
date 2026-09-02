use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use tuff_hooks_spec::{CompatibilityMatrix, CoverageLevel};

use crate::error::{Result, TuffError};
use crate::manifest::{CapabilityManifest, CapabilityType, HookConfig};

/// Append hook groups that this event does not already register.
///
/// `tuff add` is re-runnable and a pack may be installed over an existing
/// install, so the same hook fragment is merged more than once. Appending
/// unconditionally leaves a duplicate group behind on every re-add, and the
/// harness then runs that hook once per copy.
pub fn extend_hook_groups(existing: &mut Vec<serde_json::Value>, additions: &[serde_json::Value]) {
    for addition in additions {
        if !existing.iter().any(|group| group == addition) {
            existing.push(addition.clone());
        }
    }
}

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
pub enum HookRenderDiagnosticLevel {
    Warning,
}

#[derive(Debug, Clone)]
pub struct HookRenderDiagnostic {
    pub level: HookRenderDiagnosticLevel,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct HookRenderContext<'a> {
    pub capability_id: &'a str,
    pub hook: &'a HookConfig,
    pub source_files: &'a [(String, Vec<u8>)],
    pub repo_root: &'a Path,
    pub track_managed_hooks: bool,
}

#[derive(Debug, Clone)]
pub struct HookRenderPlan {
    pub files: Vec<PlannedFile>,
    pub managed_hooks: Vec<crate::lockfile::ManagedHook>,
    pub diagnostics: Vec<HookRenderDiagnostic>,
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
    McpServer {
        server: crate::manifest::McpServerConfig,
    },
}

impl CapabilityKind {
    pub fn capability_type(&self) -> CapabilityType {
        match self {
            Self::Skill => CapabilityType::Skill,
            Self::Tool { .. } => CapabilityType::Tool,
            Self::Hook { .. } => CapabilityType::Hook,
            Self::Workflow { .. } => CapabilityType::Workflow,
            Self::McpServer { .. } => CapabilityType::McpServer,
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

#[derive(Serialize)]
struct WorkflowDocument<'a> {
    id: &'a str,
    version: &'a str,
    #[serde(rename = "type")]
    capability_type: CapabilityType,
    description: &'a str,
    workflow: &'a crate::manifest::WorkflowConfig,
}

pub fn resolve_capability(manifest: &CapabilityManifest) -> Result<ResolvedCapability> {
    let source_files = manifest.read_source_contents_with_names()?;
    let kind =
        match manifest.capability_type {
            CapabilityType::Skill => CapabilityKind::Skill,
            CapabilityType::Tool => CapabilityKind::Tool {
                parameters: manifest.parameters.clone().ok_or_else(|| {
                    TuffError::usage("tool capability requires [parameters] section")
                })?,
                implementation: manifest.implementation.clone().ok_or_else(|| {
                    TuffError::usage("tool capability requires [implementation] section")
                })?,
            },
            CapabilityType::Hook => {
                CapabilityKind::Hook {
                    hook: HookDefinition::Command(manifest.hook.clone().ok_or_else(|| {
                        TuffError::usage("hook capability requires [hook] section")
                    })?),
                }
            }
            CapabilityType::Workflow => CapabilityKind::Workflow {
                workflow: manifest.workflow.clone().ok_or_else(|| {
                    TuffError::usage("workflow capability requires [workflow] section")
                })?,
            },
            CapabilityType::Policy => {
                return Err(TuffError::unsupported(
                    "policy capabilities are not installable yet",
                ));
            }
            CapabilityType::McpServer => CapabilityKind::McpServer {
                server: manifest.server.clone().ok_or_else(|| {
                    TuffError::usage("mcp-server capability requires [server] section")
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
    fn hook_compatibility(&self) -> &'static CompatibilityMatrix;
    fn hook_settings_relpath(&self) -> &'static str;
    fn scaffold_hook_event(&self) -> &'static str;
    fn hook_filename(&self) -> &'static str;
    fn hook_file_content(&self, hook_cfg: &crate::manifest::HookConfig) -> Result<Vec<u8>> {
        render_hook_script(hook_cfg)
    }
    fn render_standard_hook(&self, context: HookRenderContext<'_>) -> Result<HookRenderPlan> {
        let matrix = self.hook_compatibility();
        let Some(entry) = matrix.find_event(&context.hook.event) else {
            return Err(TuffError::unsupported(format!(
                "{} does not support hook event '{}'. Supported events: {}",
                self.display_name(),
                context.hook.event,
                matrix.supported_native_events().join(", ")
            )));
        };
        let Some(native_event) = entry.native_event_name() else {
            let suffix = entry
                .caveat
                .map(|caveat| format!(": {caveat}"))
                .unwrap_or_default();
            return Err(TuffError::unsupported(format!(
                "{} does not support hook event '{}'{}",
                self.display_name(),
                context.hook.event,
                suffix
            )));
        };

        let command = format!(
            "sh {}/hooks/{}/{}",
            self.dir_prefix(),
            context.capability_id,
            self.hook_filename()
        );
        let target_path = context
            .repo_root
            .join(self.dir_prefix())
            .join("hooks")
            .join(context.capability_id)
            .join(self.hook_filename());
        let script = self.hook_file_content(context.hook)?;
        let settings_relpath = self.hook_settings_relpath();
        let fragment = self.command_hook_fragment(native_event, &command);
        let settings_path = context.repo_root.join(settings_relpath);
        let existing = if settings_path.is_file() {
            Some(std::fs::read(&settings_path)?)
        } else {
            None
        };
        let merged = self.merge_hook_fragment(existing.as_deref(), &fragment)?;

        let mut files = vec![PlannedFile::new(
            relative_or_absolute_fs(&target_path, context.repo_root),
            script,
        )];
        for (relative, content) in context.source_files {
            let path = context
                .repo_root
                .join(self.dir_prefix())
                .join("hooks")
                .join(context.capability_id)
                .join(relative);
            files.push(PlannedFile::new(
                relative_or_absolute_fs(&path, context.repo_root),
                content.clone(),
            ));
        }
        files.push(PlannedFile::mergeable(
            relative_or_absolute_fs(&settings_path, context.repo_root),
            merged,
        ));

        let mut diagnostics = Vec::new();
        if entry.coverage == CoverageLevel::Partial {
            let scope = if entry.scope.is_empty() {
                "partial coverage".to_string()
            } else {
                format!("scope: {}", entry.scope.join(", "))
            };
            let caveat = entry
                .caveat
                .map(|caveat| format!("; {caveat}"))
                .unwrap_or_default();
            diagnostics.push(HookRenderDiagnostic {
                level: HookRenderDiagnosticLevel::Warning,
                message: format!(
                    "{} renders '{}' with partial compatibility ({scope}{caveat})",
                    self.display_name(),
                    entry.event
                ),
            });
        }

        let managed_hooks = if context.track_managed_hooks {
            crate::lockfile::managed_hooks_from_fragment_with_canonical(
                context.repo_root,
                settings_relpath,
                &fragment,
                Some(entry.event.as_str()),
            )?
        } else {
            Vec::new()
        };

        Ok(HookRenderPlan {
            files,
            managed_hooks,
            diagnostics,
        })
    }
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
            return Err(TuffError::unsupported(format!(
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
            TuffError::unsupported(format!(
                "{} does not support hook event '{}'{}",
                self.display_name(),
                raw_event,
                suffix
            ))
        })
    }

    fn canonical_hook_event(&self, raw_event: &str) -> Result<&'static str> {
        let matrix = self.hook_compatibility();
        let Some(entry) = matrix.find_event(raw_event) else {
            return Err(TuffError::unsupported(format!(
                "{} does not support hook event '{}'",
                self.display_name(),
                raw_event
            )));
        };
        entry
            .coverage
            .is_supported()
            .then_some(entry.event.as_str())
            .ok_or_else(|| {
                let suffix = entry
                    .caveat
                    .map(|caveat| format!(": {caveat}"))
                    .unwrap_or_default();
                TuffError::unsupported(format!(
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

    /// How this harness spells a reference to an environment variable inside
    /// its MCP config. Claude Code and most stdio clients expand `${VAR}`.
    fn mcp_env_reference(&self, var: &str) -> String {
        format!("${{{var}}}")
    }

    /// The `mcpServers.<id>` entry this harness needs for an external MCP
    /// server. Secrets are emitted as env references, never values.
    fn mcp_server_entry(&self, server: &crate::manifest::McpServerConfig) -> serde_json::Value {
        use crate::manifest::McpTransport;
        match server.transport {
            McpTransport::Stdio => {
                let mut entry = serde_json::json!({
                    "command": server.command.clone().unwrap_or_default(),
                    "args": server.args,
                });
                if !server.env.is_empty() {
                    let env: serde_json::Map<String, serde_json::Value> = server
                        .env
                        .iter()
                        .map(|(name, reference)| {
                            (
                                name.clone(),
                                serde_json::Value::String(
                                    self.mcp_env_reference(&reference.from_env),
                                ),
                            )
                        })
                        .collect();
                    entry["env"] = serde_json::Value::Object(env);
                }
                entry
            }
            McpTransport::Http => serde_json::json!({
                "type": "http",
                "url": server.url.clone().unwrap_or_default(),
            }),
        }
    }

    fn plan(&self, capability: &ResolvedCapability, repo_root: &Path) -> Result<Vec<PlannedFile>> {
        match capability.capability_type {
            CapabilityType::Tool => self.plan_tool(capability, repo_root),
            CapabilityType::Hook => self.plan_hook(capability, repo_root),
            CapabilityType::Workflow => self.plan_workflow(capability, repo_root),
            CapabilityType::McpServer => self.plan_mcp_server(capability, repo_root),
            CapabilityType::Policy => Err(TuffError::unsupported(
                "policy capabilities are not installable yet",
            )),
            CapabilityType::Skill => self.plan_skill(capability, repo_root),
        }
    }

    fn remove(
        &self,
        primitive_id: &str,
        repo_root: &Path,
        managed_hooks: &[crate::lockfile::ManagedHook],
    ) -> Result<()> {
        let prefix = self.dir_prefix();
        for kind in &["skills", "tools", "hooks", "workflows", "mcp-servers"] {
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
            return Err(TuffError::usage("no source files to emit"));
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
            return Err(TuffError::new("plan_hook called on non-hook capability"));
        };

        match hook {
            HookDefinition::Command(hook_cfg) => {
                let render = self.render_standard_hook(HookRenderContext {
                    capability_id: &capability.id,
                    hook: hook_cfg,
                    source_files: &capability.source_files,
                    repo_root,
                    track_managed_hooks: false,
                })?;
                Ok(render.files)
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
            return Err(TuffError::new(
                "plan_workflow called on non-workflow capability",
            ));
        };

        let target_path = repo_root
            .join(self.dir_prefix())
            .join("workflows")
            .join(&capability.id)
            .join("workflow.toml");

        let content = serialize_workflow(capability, wf)?;

        Ok(vec![PlannedFile::new(
            relative_or_absolute_fs(&target_path, repo_root),
            content,
        )])
    }

    /// Emit the canonical `server.toml` record. The JSON entry in the
    /// harness's MCP config is the artifact the harness reads; this file is
    /// what gives the capability a tree to hash, so `check`/`diff`/`delete`
    /// work exactly as they do for every other kind.
    fn plan_mcp_server(
        &self,
        capability: &ResolvedCapability,
        repo_root: &Path,
    ) -> Result<Vec<PlannedFile>> {
        let CapabilityKind::McpServer { server } = &capability.kind else {
            return Err(TuffError::new(
                "plan_mcp_server called on non-mcp-server capability",
            ));
        };

        let target_path = repo_root
            .join(self.dir_prefix())
            .join("mcp-servers")
            .join(&capability.id)
            .join("server.toml");

        let content = serialize_mcp_server(capability, server)?;

        Ok(vec![PlannedFile::new(
            relative_or_absolute_fs(&target_path, repo_root),
            content,
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

fn render_hook_script(hook_cfg: &HookConfig) -> Result<Vec<u8>> {
    let working_directory = shell_single_quote(&hook_cfg.working_directory)?;
    let command = shell_single_quote(&hook_cfg.command)?;
    Ok(format!(
        "#!/usr/bin/env bash\nset -euo pipefail\ncd -- {working_directory}\nexec bash -euo pipefail -c {command}\n"
    )
    .into_bytes())
}

fn shell_single_quote(value: &str) -> Result<String> {
    if value.contains('\0') {
        return Err(TuffError::usage(
            "hook working directory and command cannot contain NUL bytes",
        ));
    }
    Ok(format!("'{}'", value.replace('\'', "'\"'\"'")))
}

fn serialize_workflow(
    capability: &ResolvedCapability,
    workflow: &crate::manifest::WorkflowConfig,
) -> Result<Vec<u8>> {
    let document = WorkflowDocument {
        id: &capability.id,
        version: &capability.version,
        capability_type: capability.capability_type,
        description: &capability.description,
        workflow,
    };
    let mut content = toml::to_string_pretty(&document)?;
    if !content.ends_with('\n') {
        content.push('\n');
    }
    Ok(content.into_bytes())
}

#[derive(Serialize)]
struct McpServerDocument<'a> {
    id: &'a str,
    version: &'a str,
    #[serde(rename = "type")]
    capability_type: CapabilityType,
    description: &'a str,
    server: &'a crate::manifest::McpServerConfig,
}

fn serialize_mcp_server(
    capability: &ResolvedCapability,
    server: &crate::manifest::McpServerConfig,
) -> Result<Vec<u8>> {
    let document = McpServerDocument {
        id: &capability.id,
        version: &capability.version,
        capability_type: capability.capability_type,
        description: &capability.description,
        server,
    };
    let mut content = toml::to_string_pretty(&document)?;
    if !content.ends_with('\n') {
        content.push('\n');
    }
    Ok(content.into_bytes())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Requirement, WorkflowConfig};

    #[cfg(unix)]
    #[test]
    fn hook_script_preserves_shell_sensitive_values() {
        use std::process::Command;

        let temp = tempfile::tempdir().expect("tempdir");
        let working_directory = temp.path().join("directory with ' quote");
        std::fs::create_dir(&working_directory).expect("create working directory");
        let hook = HookConfig {
            event: "stop".to_string(),
            command: "printf '%s\\n' 'safe; $HOME `literal`' > result.txt".to_string(),
            working_directory: working_directory.to_string_lossy().into_owned(),
        };
        let script_path = temp.path().join("run.sh");
        std::fs::write(
            &script_path,
            render_hook_script(&hook).expect("render script"),
        )
        .expect("write script");

        let syntax = Command::new("bash")
            .arg("-n")
            .arg(&script_path)
            .status()
            .expect("check script syntax");
        assert!(syntax.success());
        let executed = Command::new("bash")
            .arg(&script_path)
            .status()
            .expect("execute script");
        assert!(executed.success());
        assert_eq!(
            std::fs::read_to_string(working_directory.join("result.txt"))
                .expect("read command output"),
            "safe; $HOME `literal`\n"
        );
    }

    #[test]
    fn hook_script_rejects_nul_bytes() {
        let hook = HookConfig {
            event: "stop".to_string(),
            command: "printf '\0'".to_string(),
            working_directory: ".".to_string(),
        };

        assert!(render_hook_script(&hook).is_err());
    }

    #[test]
    fn workflow_serialization_escapes_manifest_values() {
        let workflow = WorkflowConfig {
            requires: vec![Requirement {
                id: "dependency\"\\name".to_string(),
                capability_type: CapabilityType::Skill,
            }],
        };
        let capability = ResolvedCapability {
            id: "workflow\"id".to_string(),
            capability_type: CapabilityType::Workflow,
            version: "1.0.0".to_string(),
            description: "first line\nsecond \"line\" \\ value".to_string(),
            source_files: Vec::new(),
            source_dir: PathBuf::new(),
            kind: CapabilityKind::Workflow {
                workflow: workflow.clone(),
            },
        };

        let bytes = serialize_workflow(&capability, &workflow).expect("serialize workflow");
        let parsed: toml::Value = toml::from_slice(&bytes).expect("parse emitted workflow");

        assert_eq!(parsed["id"].as_str(), Some("workflow\"id"));
        assert_eq!(
            parsed["description"].as_str(),
            Some("first line\nsecond \"line\" \\ value")
        );
        assert_eq!(
            parsed["workflow"]["requires"][0]["id"].as_str(),
            Some("dependency\"\\name")
        );
    }
}
