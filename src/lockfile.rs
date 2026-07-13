use std::{
    collections::BTreeMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use similar::{ChangeTag, TextDiff};

use crate::adapter::EmittedFile;
use crate::error::{CoralError, Result};

pub const LOCKFILE_VERSION: u8 = 2;

#[derive(Debug, Serialize, Deserialize)]
pub struct Lockfile {
    pub version: u8,
    #[serde(rename = "capabilities", alias = "primitives")]
    pub capabilities: BTreeMap<String, CapabilityLockEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityLockEntry {
    #[serde(rename = "type", alias = "primitive")]
    pub capability_type: String,
    #[serde(rename = "installedVersion")]
    pub installed_version: String,
    #[serde(rename = "sourcePath")]
    pub source_path: String,
    pub targets: BTreeMap<String, TargetLockEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceMetadata>,
    #[serde(default = "default_scope")]
    pub scope: String,
}

fn default_scope() -> String {
    "project".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMetadata {
    #[serde(rename = "type")]
    pub source_type: String,
    pub url: String,
    #[serde(rename = "ref")]
    pub source_ref: String,
    pub skill: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetLockEntry {
    #[serde(rename = "baselineDir")]
    pub baseline_dir: String,
    #[serde(rename = "emittedFiles")]
    pub emitted_files: Vec<EmittedFile>,
}

pub fn lockfile_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".coral").join("coral-lock.json")
}

pub fn init_lockfile(repo_root: &Path) -> Result<PathBuf> {
    let coral_dir = repo_root.join(".coral");
    let lock_path = coral_dir.join("coral-lock.json");
    init_lockfile_at(&lock_path)?;
    Ok(lock_path)
}

pub fn init_lockfile_at(lock_path: &Path) -> Result<()> {
    if !lock_path.exists() {
        write_lockfile_at(
            lock_path,
            &Lockfile {
                version: LOCKFILE_VERSION,
                capabilities: BTreeMap::new(),
            },
        )?;
    }
    Ok(())
}

pub fn require_lockfile(repo_root: &Path) -> Result<Lockfile> {
    let lock_path = lockfile_path(repo_root);
    read_lockfile_at(&lock_path)
}

pub fn read_lockfile_at(path: &Path) -> Result<Lockfile> {
    if !path.exists() {
        let parent = path.parent().unwrap_or(Path::new("."));
        return Err(CoralError::new(format!(
            "{} is missing; run 'coral init' first",
            parent
                .join(path.file_name().unwrap_or(OsStr::new("coral-lock.json")))
                .display()
        )));
    }

    let lockfile: Lockfile = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    if lockfile.version != LOCKFILE_VERSION {
        return Err(CoralError::new(format!(
            "unsupported lockfile version: {}",
            lockfile.version
        )));
    }
    Ok(lockfile)
}

pub fn write_lockfile(repo_root: &Path, lockfile: &Lockfile) -> Result<()> {
    let lock_path = lockfile_path(repo_root);
    write_lockfile_at(&lock_path, lockfile)
}

pub fn write_lockfile_at(path: &Path, lockfile: &Lockfile) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(lockfile)? + "\n")?;
    Ok(())
}

pub fn hash_bytes(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}

pub fn drift_status(repo_root: &Path, emitted_file: &EmittedFile) -> &'static str {
    let target_path = repo_root.join(&emitted_file.path);
    if !target_path.exists() {
        return "missing";
    }

    let Ok(content) = std::fs::read(&target_path) else {
        return "missing";
    };

    if hash_bytes(&content) == emitted_file.hash {
        "clean"
    } else {
        "modified"
    }
}

pub fn diff_against_baseline(
    repo_root: &Path,
    primitive_id: &str,
    target_id: &str,
    entry: &CapabilityLockEntry,
) -> Result<String> {
    let target_entry = entry.targets.get(target_id).ok_or_else(|| {
        CoralError::new(format!(
            "no target '{}' recorded for primitive '{}'",
            target_id, primitive_id
        ))
    })?;

    let baseline_dir = repo_root.join(&target_entry.baseline_dir);

    let mut output = String::new();
    for emitted in &target_entry.emitted_files {
        let file_name = Path::new(&emitted.path)
            .file_name()
            .ok_or_else(|| {
                CoralError::new(format!("invalid emitted file path: {}", emitted.path))
            })?;
        let baseline_path = baseline_dir.join(file_name);
        let target_path = repo_root.join(&emitted.path);

        if !baseline_path.exists() {
            return Err(CoralError::new(format!(
                "baseline file missing for '{}': {}",
                primitive_id,
                baseline_path.display()
            )));
        }
        if !target_path.exists() {
            return Err(CoralError::new(format!(
                "installed file missing for '{}': {}",
                primitive_id,
                target_path.display()
            )));
        }

        let baseline = std::fs::read_to_string(&baseline_path)?;
        let target = std::fs::read_to_string(&target_path)?;
        if baseline == target {
            continue;
        }

        let diff = TextDiff::from_lines(&baseline, &target);
        output.push_str(&format!(
            "--- baseline/{target_id}/{primitive_id}/\n+++ {}\n",
            emitted.path
        ));
        for group in diff.grouped_ops(3) {
            for operation in group {
                for change in diff.iter_changes(&operation) {
                    let sign = match change.tag() {
                        ChangeTag::Delete => "-",
                        ChangeTag::Insert => "+",
                        ChangeTag::Equal => " ",
                    };
                    output.push_str(sign);
                    output.push_str(change.value());
                }
            }
        }
    }

    Ok(output)
}

pub fn relative_or_absolute_fs(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}

pub fn absolutize(repo_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn init_lockfile_at_creates_new_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test-lock.json");
        init_lockfile_at(&path).unwrap();
        assert!(path.exists());

        let lf = read_lockfile_at(&path).unwrap();
        assert_eq!(lf.version, 2);
        assert!(lf.capabilities.is_empty());
    }

    #[test]
    fn read_lockfile_at_rejects_missing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nonexistent.json");
        assert!(read_lockfile_at(&path).is_err());
    }

    #[test]
    fn write_and_read_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("roundtrip.json");
        let mut lf = Lockfile {
            version: 2,
            capabilities: BTreeMap::new(),
        };
        lf.capabilities.insert(
            "test".into(),
            CapabilityLockEntry {
                capability_type: "skill".into(),
                installed_version: "1.0".into(),
                source_path: "".into(),
                targets: BTreeMap::new(),
                source: None,
                scope: "project".into(),
            },
        );
        write_lockfile_at(&path, &lf).unwrap();
        let read = read_lockfile_at(&path).unwrap();
        assert_eq!(read.capabilities.len(), 1);
    }

    #[test]
    fn read_lockfile_accepts_legacy_primitives_schema() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("legacy-lock.json");
        fs::write(
            &path,
            r#"{
  "version": 2,
  "primitives": {
    "test": {
      "primitive": "skill",
      "installedVersion": "1.0",
      "sourcePath": "examples/test",
      "scope": "project",
      "targets": {}
    }
  }
}
"#,
        )
        .unwrap();

        let read = read_lockfile_at(&path).unwrap();
        let entry = read.capabilities.get("test").unwrap();
        assert_eq!(entry.capability_type, "skill");
        assert_eq!(entry.installed_version, "1.0");
    }

    #[test]
    fn hash_bytes_produces_consistent_output() {
        let h1 = hash_bytes(b"hello");
        let h2 = hash_bytes(b"hello");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
        assert_ne!(h1, hash_bytes(b"world"));
    }

    #[test]
    fn drift_status_reports_clean() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.md");
        fs::write(&file, "content").unwrap();

        let emitted = crate::adapter::EmittedFile {
            path: file.file_name().unwrap().to_string_lossy().to_string(),
            hash: hash_bytes(b"content"),
        };
        assert_eq!(drift_status(tmp.path(), &emitted), "clean");
    }

    #[test]
    fn drift_status_reports_modified() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.md");
        fs::write(&file, "different").unwrap();

        let emitted = crate::adapter::EmittedFile {
            path: file.file_name().unwrap().to_string_lossy().to_string(),
            hash: hash_bytes(b"original"),
        };
        assert_eq!(drift_status(tmp.path(), &emitted), "modified");
    }

    #[test]
    fn drift_status_reports_missing() {
        let tmp = TempDir::new().unwrap();
        let emitted = crate::adapter::EmittedFile {
            path: "nonexistent.md".into(),
            hash: "abc".into(),
        };
        assert_eq!(drift_status(tmp.path(), &emitted), "missing");
    }
}
