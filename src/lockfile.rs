use std::{
    collections::{BTreeMap, BTreeSet},
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
    #[serde(rename = "capabilities")]
    pub capabilities: BTreeMap<String, CapabilityLockEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityLockEntry {
    #[serde(rename = "type")]
    pub capability_type: String,
    #[serde(rename = "installedVersion")]
    pub installed_version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
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
    #[serde(rename = "emittedFiles")]
    pub emitted_files: Vec<EmittedFile>,
    #[serde(default)]
    pub ownership: TargetOwnership,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetOwnership {
    #[default]
    Generated,
    Imported,
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

fn object_path_for_hash(repo_root: &Path, hash: &str) -> Result<PathBuf> {
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(CoralError::new(format!("invalid baseline hash: {hash}")));
    }
    let (prefix, suffix) = hash.split_at(2);
    Ok(repo_root
        .join(".coral")
        .join("objects")
        .join("sha256")
        .join(prefix)
        .join(suffix))
}

pub fn write_baseline_object(repo_root: &Path, content: &[u8]) -> Result<String> {
    let hash = hash_bytes(content);
    let object_path = object_path_for_hash(repo_root, &hash)?;
    if object_path.exists() {
        let existing = std::fs::read(&object_path)?;
        if existing != content {
            return Err(CoralError::new(format!(
                "baseline object hash collision or corruption: {}",
                object_path.display()
            )));
        }
        return Ok(hash);
    }

    if let Some(parent) = object_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(object_path, content)?;
    Ok(hash)
}

pub fn read_baseline_object(repo_root: &Path, hash: &str) -> Result<Vec<u8>> {
    let object_path = object_path_for_hash(repo_root, hash)?;
    if !object_path.is_file() {
        return Err(CoralError::new(format!(
            "baseline object missing: {}",
            object_path.display()
        )));
    }
    let content = std::fs::read(&object_path)?;
    let actual = hash_bytes(&content);
    if actual != hash {
        return Err(CoralError::new(format!(
            "baseline object hash mismatch: {}",
            object_path.display()
        )));
    }
    Ok(content)
}

pub fn prune_unreferenced_baseline_objects(repo_root: &Path, lockfile: &Lockfile) -> Result<usize> {
    let objects_root = repo_root.join(".coral").join("objects").join("sha256");
    if !objects_root.exists() {
        return Ok(0);
    }

    let referenced: BTreeSet<&str> = lockfile
        .capabilities
        .values()
        .flat_map(|entry| entry.targets.values())
        .flat_map(|target| target.emitted_files.iter())
        .map(|emitted| emitted.baseline_hash.as_str())
        .collect();

    let mut removed = 0usize;
    for prefix_entry in std::fs::read_dir(&objects_root)? {
        let prefix_entry = prefix_entry?;
        let prefix_path = prefix_entry.path();
        if !prefix_path.is_dir() {
            continue;
        }
        let Some(prefix) = prefix_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        for object_entry in std::fs::read_dir(&prefix_path)? {
            let object_entry = object_entry?;
            let object_path = object_entry.path();
            if !object_path.is_file() {
                continue;
            }
            let Some(suffix) = object_path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let hash = format!("{prefix}{suffix}");
            if !referenced.contains(hash.as_str()) {
                std::fs::remove_file(&object_path)?;
                removed += 1;
            }
        }
        if std::fs::read_dir(&prefix_path)?.next().is_none() {
            std::fs::remove_dir(&prefix_path)?;
        }
    }

    Ok(removed)
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

    let mut output = String::new();
    for emitted in &target_entry.emitted_files {
        let rel_path = capability_relative_path(&emitted.path, primitive_id);
        let target_path = repo_root.join(&emitted.path);

        if !target_path.exists() {
            return Err(CoralError::new(format!(
                "installed file missing for '{}': {}",
                primitive_id,
                target_path.display()
            )));
        }

        let baseline = String::from_utf8(read_baseline_object(repo_root, &emitted.baseline_hash)?)
            .map_err(|error| {
                CoralError::new(format!(
                    "baseline object is not valid UTF-8 for '{}': {}",
                    primitive_id, error
                ))
            })?;
        let target = std::fs::read_to_string(&target_path)?;
        if baseline == target {
            continue;
        }

        let diff = TextDiff::from_lines(&baseline, &target);
        output.push_str(&format!(
            "--- baseline/{target_id}/{primitive_id}/{}\n+++ {}\n",
            rel_path.display(),
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

fn capability_relative_path(path: &str, capability_id: &str) -> PathBuf {
    for base in [
        ".agents/skills",
        ".claude/skills",
        ".agents/tools",
        ".claude/tools",
        ".agents/hooks",
        ".claude/hooks",
        ".agents/workflows",
        ".claude/workflows",
    ] {
        let prefix = format!("{base}/{capability_id}/");
        if let Some(rel) = path.strip_prefix(&prefix) {
            return PathBuf::from(rel);
        }
    }

    Path::new(path)
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(path))
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
    fn read_lockfile_at_rejects_v3_schema() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("v3-lock.json");
        fs::write(
            &path,
            r#"{
  "version": 3,
  "capabilities": {}
}
"#,
        )
        .unwrap();

        let error = read_lockfile_at(&path).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unsupported lockfile version: 3")
        );
    }

    #[test]
    fn write_and_read_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("roundtrip.json");
        let mut lf = Lockfile {
            version: LOCKFILE_VERSION,
            capabilities: BTreeMap::new(),
        };
        lf.capabilities.insert(
            "test".into(),
            CapabilityLockEntry {
                capability_type: "skill".into(),
                installed_version: "1.0".into(),
                description: "test skill".into(),
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
    fn missing_target_ownership_defaults_to_generated() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("legacy.json");
        std::fs::write(
            &path,
            r#"{
  "version": 2,
  "capabilities": {
    "legacy": {
      "type": "skill",
      "installedVersion": "1.0",
      "sourcePath": "legacy",
      "targets": {
        "open-agents": {
          "emittedFiles": []
        }
      }
    }
  }
}"#,
        )
        .unwrap();

        let read = read_lockfile_at(&path).unwrap();
        assert_eq!(
            read.capabilities["legacy"].targets["open-agents"].ownership,
            TargetOwnership::Generated
        );
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
            baseline_hash: hash_bytes(b"content"),
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
            baseline_hash: hash_bytes(b"original"),
        };
        assert_eq!(drift_status(tmp.path(), &emitted), "modified");
    }

    #[test]
    fn drift_status_reports_missing() {
        let tmp = TempDir::new().unwrap();
        let emitted = crate::adapter::EmittedFile {
            path: "nonexistent.md".into(),
            hash: "abc".into(),
            baseline_hash: "abc".into(),
        };
        assert_eq!(drift_status(tmp.path(), &emitted), "missing");
    }
}
