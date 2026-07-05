use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{CoralError, Result};

#[derive(Debug, Deserialize)]
pub struct CapabilityManifest {
    pub id: String,
    pub version: String,
    #[serde(rename = "type")]
    pub capability_type: String,
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
    pub runtime_deps: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HookConfig {
    pub event: String,
    pub command: String,
    #[serde(default = "default_cwd")]
    pub working_directory: String,
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
                return Err(CoralError::new(format!(
                    "capability source file not found: {}",
                    path.display()
                )));
            }
            paths.push(path);
        }

        if self.capability_type == "tool" {
            if let Some(ref imp) = self.implementation {
                let ep_path = self.root.join(&imp.entrypoint);
                if !paths.contains(&ep_path) && ep_path.exists() {
                    paths.push(ep_path);
                }
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
        return Err(CoralError::new(format!(
            "capability manifest field '{field}' must be a non-empty string"
        )));
    }
    Ok(())
}

pub fn load_manifest(capability_dir: &Path) -> Result<CapabilityManifest> {
    let manifest_path = capability_dir.join("coral.toml");
    if !manifest_path.exists() {
        return Err(CoralError::new(format!(
            "capability manifest not found: {}",
            manifest_path.display()
        )));
    }

    let mut manifest: CapabilityManifest =
        toml::from_str(&std::fs::read_to_string(&manifest_path)?)?;
    manifest.root = capability_dir.to_path_buf();

    validate_non_empty("id", &manifest.id)?;
    validate_non_empty("version", &manifest.version)?;
    validate_non_empty("type", &manifest.capability_type)?;
    validate_non_empty("description", &manifest.description)?;

    match manifest.capability_type.as_str() {
        "skill" => {
            if manifest.files.is_empty() {
                return Err(CoralError::new("skill capability 'files' must not be empty"));
            }
            manifest.source_files()?;
        }
        "tool" => {
            if manifest.parameters.is_none() {
                return Err(CoralError::new(
                    "tool capability requires a [parameters] section with JSON Schema",
                ));
            }
            if manifest.implementation.is_none() {
                return Err(CoralError::new(
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
        "hook" => {
            let hook_cfg = manifest.hook.as_ref().ok_or_else(|| {
                CoralError::new("hook capability requires a [hook] section")
            })?;

            if hook_cfg.event.trim().is_empty() {
                return Err(CoralError::new("hook 'event' must be a non-empty string"));
            }
            if hook_cfg.command.trim().is_empty() {
                return Err(CoralError::new("hook 'command' must be a non-empty string"));
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
        other => {
            return Err(CoralError::new(format!(
                "unsupported capability type '{}'; supported: skill, tool, hook",
                other
            )));
        }
    }

    Ok(manifest)
}

pub fn synthetic_manifest(skill_dir: &Path, name: &str, version: &str) -> Result<CapabilityManifest> {
    let mut files = Vec::new();
    walk_skill_dir(skill_dir, "", &mut files)?;

    Ok(CapabilityManifest {
        id: name.to_string(),
        version: version.to_string(),
        capability_type: "skill".to_string(),
        description: String::new(),
        files,
        parameters: None,
        implementation: None,
        hook: None,
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
        } else {
            files.push(rel);
        }
    }
    Ok(())
}
