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
}

impl CapabilityType {
    pub fn plural_dir(&self) -> &'static str {
        match self {
            Self::Skill => "skills",
            Self::Tool => "tools",
            Self::Hook => "hooks",
            Self::Workflow => "workflows",
            Self::Policy => "policies",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Tool => "tool",
            Self::Hook => "hook",
            Self::Workflow => "workflow",
            Self::Policy => "policy",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "skill" => Some(Self::Skill),
            "tool" => Some(Self::Tool),
            "hook" => Some(Self::Hook),
            "workflow" => Some(Self::Workflow),
            "policy" => Some(Self::Policy),
            _ => None,
        }
    }
}

impl std::fmt::Display for CapabilityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Deserialize)]
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
    #[allow(dead_code)]
    pub targets: Vec<String>,

    #[serde(skip)]
    pub root: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImplementationConfig {
    pub language: String,
    pub entrypoint: String,
    #[serde(default)]
    pub mcp: bool,
    #[serde(default)]
    pub runtime_deps: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HookConfig {
    pub event: String,
    pub command: String,
    #[serde(default = "default_cwd")]
    pub working_directory: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowConfig {
    pub requires: Vec<Requirement>,
}

#[derive(Debug, Clone, Deserialize)]
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
                return Err(TuffError::new(format!(
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
        return Err(TuffError::new(format!(
            "capability manifest field '{field}' must be a non-empty string"
        )));
    }
    Ok(())
}

pub fn load_manifest(capability_dir: &Path) -> Result<CapabilityManifest> {
    let manifest_path = capability_dir.join("tuff.toml");
    if !manifest_path.exists() {
        return Err(TuffError::new(format!(
            "capability manifest not found: {}",
            manifest_path.display()
        )));
    }

    let mut manifest: CapabilityManifest =
        toml::from_str(&std::fs::read_to_string(&manifest_path)?)?;
    manifest.root = capability_dir.to_path_buf();

    validate_non_empty("id", &manifest.id)?;
    validate_non_empty("version", &manifest.version)?;
    validate_non_empty("type", &manifest.capability_type.to_string())?;
    validate_non_empty("description", &manifest.description)?;

    match manifest.capability_type {
        CapabilityType::Skill => {
            if manifest.files.is_empty() {
                return Err(TuffError::new("skill capability 'files' must not be empty"));
            }
            manifest.source_files()?;
        }
        CapabilityType::Tool => {
            if manifest.parameters.is_none() {
                return Err(TuffError::new(
                    "tool capability requires a [parameters] section with JSON Schema",
                ));
            }
            if manifest.implementation.is_none() {
                return Err(TuffError::new(
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
                .ok_or_else(|| TuffError::new("hook capability requires a [hook] section"))?;

            if hook_cfg.event.trim().is_empty() {
                return Err(TuffError::new("hook 'event' must be a non-empty string"));
            }
            if hook_cfg.command.trim().is_empty() {
                return Err(TuffError::new("hook 'command' must be a non-empty string"));
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
                TuffError::new("workflow capability requires a [[workflow.requires]] section")
            })?;

            if wf.requires.is_empty() {
                return Err(TuffError::new(
                    "workflow 'requires' must have at least one entry",
                ));
            }

            let mut seen = std::collections::HashSet::new();
            for req in &wf.requires {
                if req.id.trim().is_empty() {
                    return Err(TuffError::new(
                        "workflow requirement 'id' must not be empty",
                    ));
                }
                if req.id == manifest.id {
                    return Err(TuffError::new("workflow cannot require itself"));
                }
                if !seen.insert(&req.id) {
                    return Err(TuffError::new(format!(
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
            return Err(TuffError::new("policy capabilities are not supported yet"));
        }
    }

    Ok(manifest)
}

pub fn synthetic_manifest(
    skill_dir: &Path,
    name: &str,
    version: &str,
) -> Result<CapabilityManifest> {
    let skill_file = skill_dir.join("SKILL.md");
    if !skill_file.exists() {
        return Err(TuffError::new(format!(
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
        targets: Vec::new(),
        root: skill_dir.to_path_buf(),
    })
}

fn walk_skill_dir(base: &Path, prefix: &str, files: &mut Vec<String>) -> Result<()> {
    for entry in std::fs::read_dir(base)? {
        let entry = entry?;
        let path = entry.path();
        let rel = if prefix.is_empty() {
            entry.file_name().to_string_lossy().to_string()
        } else {
            format!("{}/{}", prefix, entry.file_name().to_string_lossy())
        };
        if path.is_dir() {
            walk_skill_dir(&path, &rel, files)?;
        } else if rel != "tuff.toml" {
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
}
