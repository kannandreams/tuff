use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{CoralError, Result};

#[derive(Debug, Deserialize)]
pub struct PrimitiveManifest {
    pub id: String,
    pub version: String,
    pub primitive: String,
    pub description: String,
    pub files: Vec<String>,

    #[serde(skip)]
    pub root: PathBuf,
}

impl PrimitiveManifest {
    pub fn source_files(&self) -> Result<Vec<PathBuf>> {
        if self.files.is_empty() {
            return Err(CoralError::new("manifest 'files' must not be empty"));
        }

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

pub fn load_manifest(capability_dir: &Path) -> Result<PrimitiveManifest> {
    let manifest_path = capability_dir.join("coral.toml");
    if !manifest_path.exists() {
        return Err(CoralError::new(format!(
            "capability manifest not found: {}",
            manifest_path.display()
        )));
    }

    let mut manifest: PrimitiveManifest =
        toml::from_str(&std::fs::read_to_string(&manifest_path)?)?;
    manifest.root = capability_dir.to_path_buf();

    validate_non_empty("id", &manifest.id)?;
    validate_non_empty("version", &manifest.version)?;
    validate_non_empty("primitive", &manifest.primitive)?;
    validate_non_empty("description", &manifest.description)?;

    manifest.source_files()?;

    Ok(manifest)
}

pub fn synthetic_manifest(skill_dir: &Path, name: &str, version: &str) -> Result<PrimitiveManifest> {
    let mut files = Vec::new();
    walk_skill_dir(skill_dir, "", &mut files)?;

    Ok(PrimitiveManifest {
        id: name.to_string(),
        version: version.to_string(),
        primitive: "skill".to_string(),
        description: String::new(),
        files,
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
