use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, TuffError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CapabilityType {
    Skill,
    Tool,
    Hook,
    Workflow,
    Policy,
    /// An external MCP server Tuff wires into each harness's native MCP
    /// config. Distinct from a `tool` with `implementation.mcp = true`,
    /// whose server code Tuff ships itself.
    #[serde(rename = "mcp-server")]
    McpServer,
}

impl CapabilityType {
    pub fn plural_dir(&self) -> &'static str {
        match self {
            Self::Skill => "skills",
            Self::Tool => "tools",
            Self::Hook => "hooks",
            Self::Workflow => "workflows",
            Self::Policy => "policies",
            Self::McpServer => "mcp-servers",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Tool => "tool",
            Self::Hook => "hook",
            Self::Workflow => "workflow",
            Self::Policy => "policy",
            Self::McpServer => "mcp-server",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "skill" => Some(Self::Skill),
            "tool" => Some(Self::Tool),
            "hook" => Some(Self::Hook),
            "workflow" => Some(Self::Workflow),
            "policy" => Some(Self::Policy),
            "mcp-server" | "mcp" => Some(Self::McpServer),
            _ => None,
        }
    }
}

impl std::fmt::Display for CapabilityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityManifest {
    pub id: String,
    pub version: String,
    #[serde(rename = "type")]
    pub capability_type: CapabilityType,
    pub description: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub parameters: Option<serde_json::Value>,
    #[serde(default)]
    pub implementation: Option<ImplementationConfig>,
    #[serde(default)]
    pub hook: Option<HookConfig>,
    #[serde(default)]
    pub workflow: Option<WorkflowConfig>,
    #[serde(default)]
    pub server: Option<McpServerConfig>,
    #[serde(default)]
    #[allow(dead_code)]
    pub targets: Vec<String>,

    #[serde(skip)]
    pub root: PathBuf,
}

/// Declaration of an external MCP server (`type = "mcp-server"`).
///
/// Secrets never appear here: every `[server.env]` value must be an
/// [`EnvRef`] naming the variable to read on the developer's machine, so a
/// manifest can be committed and shared without leaking anything.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    #[serde(default)]
    pub transport: McpTransport,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, EnvRef>,
    #[serde(default)]
    pub metadata: Option<McpServerMetadata>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    #[default]
    Stdio,
    Http,
}

impl McpTransport {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
        }
    }
}

/// A reference to an environment variable on the machine running the
/// harness. Deliberately the only shape an env value can take — a bare
/// string literal is rejected at parse time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvRef {
    pub from_env: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpServerMetadata {
    #[serde(default)]
    pub tools_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationConfig {
    pub language: String,
    pub entrypoint: String,
    #[serde(default)]
    pub mcp: bool,
    #[serde(default)]
    pub runtime_deps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    pub event: String,
    pub command: String,
    #[serde(default = "default_cwd")]
    pub working_directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    pub requires: Vec<Requirement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirement {
    pub id: String,
    #[serde(rename = "type")]
    pub capability_type: CapabilityType,
}

fn default_cwd() -> String {
    ".".to_string()
}

impl CapabilityManifest {
    pub fn source_files(&self) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();

        for f in &self.files {
            let clean = f.trim_start_matches("./");
            let path = self.root.join(clean);
            if !path.exists() {
                return Err(TuffError::not_found(format!(
                    "capability source file not found: {}",
                    path.display()
                )));
            }
            paths.push(path);
        }

        if self.capability_type == CapabilityType::Tool
            && let Some(ref imp) = self.implementation
        {
            let ep_path = self.root.join(&imp.entrypoint);
            if !paths.contains(&ep_path) && ep_path.exists() {
                paths.push(ep_path);
            }
        }

        Ok(paths)
    }

    pub fn read_source_contents_with_names(&self) -> Result<Vec<(String, Vec<u8>)>> {
        self.source_files()?
            .iter()
            .map(|p| {
                let rel = p
                    .strip_prefix(&self.root)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .replace('\\', "/");
                let rel = rel.strip_prefix("src/").unwrap_or(&rel).to_string();
                let content = std::fs::read(p)?;
                Ok((rel, content))
            })
            .collect()
    }
}

fn validate_non_empty(field: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(TuffError::usage(format!(
            "capability manifest field '{field}' must be a non-empty string"
        )));
    }
    Ok(())
}

pub fn load_manifest(capability_dir: &Path) -> Result<CapabilityManifest> {
    let manifest_path = capability_dir.join("tuff.toml");
    if !manifest_path.exists() {
        return Err(TuffError::not_found(format!(
            "capability manifest not found: {}",
            manifest_path.display()
        )));
    }

    let raw = std::fs::read_to_string(&manifest_path)?;
    let mut manifest = parse_manifest(&raw, &manifest_path)?;
    manifest.root = capability_dir.to_path_buf();

    validate_non_empty("id", &manifest.id)?;
    validate_non_empty("version", &manifest.version)?;
    validate_non_empty("type", &manifest.capability_type.to_string())?;
    validate_non_empty("description", &manifest.description)?;

    match manifest.capability_type {
        CapabilityType::Skill => {
            if manifest.files.is_empty() {
                return Err(TuffError::usage(
                    "skill capability 'files' must not be empty",
                ));
            }
            manifest.source_files()?;
        }
        CapabilityType::Tool => {
            if manifest.parameters.is_none() {
                return Err(TuffError::usage(
                    "tool capability requires a [parameters] section with JSON Schema",
                ));
            }
            if manifest.implementation.is_none() {
                return Err(TuffError::usage(
                    "tool capability requires an [implementation] section",
                ));
            }

            let params = manifest.parameters.as_ref().unwrap();
            crate::tool::validate_json_schema(params)?;

            let impl_cfg = manifest.implementation.as_ref().unwrap();
            crate::tool::validate_entrypoint(&manifest.root, &impl_cfg.entrypoint)?;

            if !impl_cfg.runtime_deps.is_empty() {
                eprintln!(
                    "note: this tool requires runtime dependencies: {}",
                    impl_cfg.runtime_deps.join(", ")
                );
            }

            if !manifest.files.is_empty() {
                manifest.source_files()?;
            }
        }
        CapabilityType::Hook => {
            let hook_cfg = manifest
                .hook
                .as_ref()
                .ok_or_else(|| TuffError::usage("hook capability requires a [hook] section"))?;

            if hook_cfg.event.trim().is_empty() {
                return Err(TuffError::usage("hook 'event' must be a non-empty string"));
            }
            if hook_cfg.command.trim().is_empty() {
                return Err(TuffError::usage(
                    "hook 'command' must be a non-empty string",
                ));
            }

            crate::tool::check_path_traversal(&hook_cfg.working_directory)?;

            eprintln!(
                "note: this hook runs '{}' on event '{}' — it will not be executed during install",
                hook_cfg.command, hook_cfg.event
            );

            if !manifest.files.is_empty() {
                manifest.source_files()?;
            }
        }
        CapabilityType::Workflow => {
            let wf = manifest.workflow.as_ref().ok_or_else(|| {
                TuffError::usage("workflow capability requires a [[workflow.requires]] section")
            })?;

            if wf.requires.is_empty() {
                return Err(TuffError::usage(
                    "workflow 'requires' must have at least one entry",
                ));
            }

            let mut seen = std::collections::HashSet::new();
            for req in &wf.requires {
                if req.id.trim().is_empty() {
                    return Err(TuffError::usage(
                        "workflow requirement 'id' must not be empty",
                    ));
                }
                if req.id == manifest.id {
                    return Err(TuffError::usage("workflow cannot require itself"));
                }
                if !seen.insert(&req.id) {
                    return Err(TuffError::usage(format!(
                        "duplicate requirement '{}' in workflow",
                        req.id
                    )));
                }
            }

            let names: Vec<_> = wf
                .requires
                .iter()
                .map(|r| format!("{} ({})", r.id, r.capability_type))
                .collect();
            eprintln!(
                "note: workflow '{}' requires {} capabilities: {}",
                manifest.id,
                names.len(),
                names.join(", ")
            );
        }
        CapabilityType::Policy => {
            return Err(TuffError::unsupported(
                "policy capabilities are not supported yet",
            ));
        }
        CapabilityType::McpServer => {
            let server = manifest.server.as_ref().ok_or_else(|| {
                TuffError::usage("mcp-server capability requires a [server] section")
            })?;
            validate_mcp_server(server)?;

            if !manifest.files.is_empty() {
                manifest.source_files()?;
            }
        }
    }

    Ok(manifest)
}

pub fn validate_mcp_server(server: &McpServerConfig) -> Result<()> {
    match server.transport {
        McpTransport::Stdio => {
            if server
                .command
                .as_deref()
                .is_none_or(|c| c.trim().is_empty())
            {
                return Err(TuffError::usage(
                    "mcp-server with transport = \"stdio\" requires a non-empty 'command'",
                ));
            }
        }
        McpTransport::Http => {
            if server.url.as_deref().is_none_or(|u| u.trim().is_empty()) {
                return Err(TuffError::usage(
                    "mcp-server with transport = \"http\" requires a non-empty 'url'",
                ));
            }
        }
    }
    for (name, reference) in &server.env {
        if name.trim().is_empty() {
            return Err(TuffError::usage("[server.env] keys must be non-empty"));
        }
        if reference.from_env.trim().is_empty() {
            return Err(TuffError::usage(format!(
                "[server.env] {name} must reference a variable: {name} = {{ from_env = \"VAR\" }}"
            )));
        }
    }
    Ok(())
}

/// Parse a manifest, turning serde's opaque "invalid type: string" failure
/// for a literal `[server.env]` value into an error that says what to write
/// instead.
fn parse_manifest(raw: &str, manifest_path: &Path) -> Result<CapabilityManifest> {
    toml::from_str(raw).map_err(|error: toml::de::Error| {
        let message = error.to_string();
        let literal_env = raw.contains("[server.env]")
            && (message.contains("invalid type: string") || message.contains("expected a table"));
        if literal_env {
            TuffError::usage(format!(
                "invalid manifest at {}: [server.env] values must be references, never \
                 literals — write NAME = {{ from_env = \"NAME\" }} ({})",
                manifest_path.display(),
                message.trim()
            ))
        } else {
            TuffError::from(error)
        }
    })
}

/// Writes a capability manifest as deterministic TOML.
///
/// # Errors
///
/// Returns an error when serialization or filesystem writing fails.
pub fn write_manifest(path: &Path, manifest: &CapabilityManifest) -> Result<()> {
    std::fs::write(path, toml::to_string_pretty(manifest)?)?;
    Ok(())
}

pub fn synthetic_manifest(
    skill_dir: &Path,
    name: &str,
    version: &str,
) -> Result<CapabilityManifest> {
    let skill_file = skill_dir.join("SKILL.md");
    if !skill_file.exists() {
        return Err(TuffError::not_found(format!(
            "skill entrypoint not found: {}",
            skill_file.display()
        )));
    }
    let mut files = Vec::new();
    walk_skill_dir(skill_dir, "", &mut files)?;
    files.sort();

    Ok(CapabilityManifest {
        id: name.to_string(),
        version: version.to_string(),
        capability_type: CapabilityType::Skill,
        description: "Installed from git source.".to_string(),
        files,
        parameters: None,
        implementation: None,
        hook: None,
        workflow: None,
        server: None,
        targets: Vec::new(),
        root: skill_dir.to_path_buf(),
    })
}

fn walk_skill_dir(base: &Path, prefix: &str, files: &mut Vec<String>) -> Result<()> {
    for entry in std::fs::read_dir(base)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(TuffError::refused(format!(
                "symbolic links are not allowed in capability sources: {}",
                path.display()
            )));
        }
        let rel = if prefix.is_empty() {
            entry.file_name().to_string_lossy().to_string()
        } else {
            format!("{}/{}", prefix, entry.file_name().to_string_lossy())
        };
        if metadata.is_dir() {
            walk_skill_dir(&path, &rel, files)?;
        } else if metadata.is_file() && rel != "tuff.toml" {
            files.push(rel);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_manifest(dir: &std::path::Path, content: &str) {
        fs::write(dir.join("tuff.toml"), content).unwrap();
    }

    #[test]
    fn load_skill_manifest_succeeds() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src").join("SKILL.md"), "# Skill").unwrap();
        write_manifest(
            tmp.path(),
            r#"id = "test"
version = "1.0.0"
type = "skill"
description = "A test skill"
files = ["src/SKILL.md"]
"#,
        );
        let m = load_manifest(tmp.path()).unwrap();
        assert_eq!(m.id, "test");
        assert_eq!(m.capability_type, CapabilityType::Skill);
    }

    #[test]
    fn load_tool_manifest_succeeds() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("run.sh"), "echo ok").unwrap();
        write_manifest(
            tmp.path(),
            r#"id = "tool1"
version = "1.0.0"
type = "tool"
description = "A test tool"
files = ["run.sh"]

[parameters]
type = "object"
required = ["x"]
[parameters.properties.x]
type = "string"
description = "x"

[implementation]
language = "bash"
entrypoint = "run.sh"
"#,
        );
        let m = load_manifest(tmp.path()).unwrap();
        assert_eq!(m.capability_type, CapabilityType::Tool);
        assert!(m.implementation.is_some());
    }

    #[test]
    fn load_hook_manifest_succeeds() {
        let tmp = TempDir::new().unwrap();
        write_manifest(
            tmp.path(),
            r#"id = "hook1"
version = "1.0.0"
type = "hook"
description = "A test hook"

[hook]
event = "before_finish"
command = "cargo test"
"#,
        );
        let m = load_manifest(tmp.path()).unwrap();
        assert_eq!(m.capability_type, CapabilityType::Hook);
        assert!(m.hook.is_some());
    }

    #[test]
    fn load_rejects_unsupported_type() {
        let tmp = TempDir::new().unwrap();
        write_manifest(
            tmp.path(),
            r#"id = "bad"
version = "1.0.0"
type = "unknown"
description = "Bad"
files = ["SKILL.md"]
"#,
        );
        assert!(load_manifest(tmp.path()).is_err());
    }

    #[test]
    fn load_rejects_missing_manifest() {
        let tmp = TempDir::new().unwrap();
        assert!(load_manifest(tmp.path()).is_err());
    }

    #[test]
    fn source_files_resolves_paths() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src").join("SKILL.md"), "skill").unwrap();
        let m = CapabilityManifest {
            id: "t".into(),
            version: "1.0".into(),
            capability_type: CapabilityType::Skill,
            description: "desc".into(),
            files: vec!["src/SKILL.md".into()],
            parameters: None,
            implementation: None,
            hook: None,
            workflow: None,
            server: None,
            targets: vec![],
            root: tmp.path().to_path_buf(),
        };
        let files = m.source_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("SKILL.md"));
    }

    #[test]
    fn source_files_rejects_missing_file() {
        let tmp = TempDir::new().unwrap();
        let m = CapabilityManifest {
            id: "t".into(),
            version: "1.0".into(),
            capability_type: CapabilityType::Skill,
            description: "desc".into(),
            files: vec!["src/MISSING.md".into()],
            parameters: None,
            implementation: None,
            hook: None,
            workflow: None,
            server: None,
            targets: vec![],
            root: tmp.path().to_path_buf(),
        };
        assert!(m.source_files().is_err());
    }

    #[test]
    fn validate_non_empty_rejects_empty() {
        assert!(validate_non_empty("id", "").is_err());
        assert!(validate_non_empty("id", "ok").is_ok());
    }

    fn load_mcp(toml_body: &str) -> Result<CapabilityManifest> {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("tuff.toml"), toml_body).unwrap();
        load_manifest(tmp.path())
    }

    const MCP_HEAD: &str =
        "id = \"srv\"\nversion = \"1.0.0\"\ntype = \"mcp-server\"\ndescription = \"d\"\n";

    #[test]
    fn mcp_server_requires_server_section() {
        let error = load_mcp(MCP_HEAD).unwrap_err().to_string();
        assert!(error.contains("requires a [server] section"), "{error}");
    }

    #[test]
    fn mcp_server_stdio_requires_command_and_http_requires_url() {
        let error = load_mcp(&format!("{MCP_HEAD}[server]\ntransport = \"stdio\"\n"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("requires a non-empty 'command'"), "{error}");
        let error = load_mcp(&format!("{MCP_HEAD}[server]\ntransport = \"http\"\n"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("requires a non-empty 'url'"), "{error}");
        let ok = load_mcp(&format!(
            "{MCP_HEAD}[server]\ntransport = \"http\"\nurl = \"https://example.test/mcp\"\n"
        ))
        .unwrap();
        assert_eq!(ok.server.unwrap().transport, McpTransport::Http);
    }

    #[test]
    fn mcp_server_env_must_be_a_reference_not_a_literal() {
        let error = load_mcp(&format!(
            "{MCP_HEAD}[server]\ncommand = \"npx\"\n[server.env]\nTOKEN = \"literal\"\n"
        ))
        .unwrap_err()
        .to_string();
        assert!(error.contains("from_env"), "{error}");

        let ok = load_mcp(&format!(
            "{MCP_HEAD}[server]\ncommand = \"npx\"\n[server.env]\nTOKEN = {{ from_env = \"MY_TOKEN\" }}\n"
        ))
        .unwrap();
        assert_eq!(ok.server.unwrap().env["TOKEN"].from_env, "MY_TOKEN");
    }

    #[test]
    fn capability_type_round_trips_the_hyphenated_name() {
        assert_eq!(CapabilityType::McpServer.as_str(), "mcp-server");
        assert_eq!(
            CapabilityType::parse("mcp-server"),
            Some(CapabilityType::McpServer)
        );
        assert_eq!(
            CapabilityType::parse("mcp"),
            Some(CapabilityType::McpServer)
        );
        let wire = toml::to_string(&Requirement {
            id: "x".into(),
            capability_type: CapabilityType::McpServer,
        })
        .unwrap();
        assert!(wire.contains("type = \"mcp-server\""), "{wire}");
    }
}
