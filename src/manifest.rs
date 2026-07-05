use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{CoralError, Result};

#[derive(Debug, Deserialize)]
pub struct PrimitiveManifest {
    pub id: String,
    pub version: String,
    pub kind: String,
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

    pub fn read_source_contents(&self) -> Result<Vec<Vec<u8>>> {
        self.source_files()?
            .iter()
            .map(|p| std::fs::read(p).map_err(Into::into))
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
    validate_non_empty("kind", &manifest.kind)?;
    validate_non_empty("description", &manifest.description)?;

    let _ = manifest.source_files()?;

    Ok(manifest)
}
