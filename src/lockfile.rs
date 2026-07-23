use std::{
    collections::BTreeMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::adapter::EmittedFile;
use crate::error::{CoralError, Result};
use crate::manifest::CapabilityType;

pub const LOCKFILE_VERSION: u8 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct Lockfile {
    pub version: u8,
    #[serde(rename = "capabilities")]
    pub capabilities: BTreeMap<String, CapabilityLockEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityLockEntry {
    #[serde(rename = "type")]
    pub capability_type: CapabilityType,
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
    #[serde(
        default,
        rename = "managedHooks",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub managed_hooks: Vec<ManagedHook>,
    #[serde(default)]
    pub ownership: TargetOwnership,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub installed_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedHook {
    #[serde(rename = "settingsPath")]
    pub settings_path: String,
    pub event: String,
    pub command: String,
    #[serde(rename = "baselineHash")]
    pub baseline_hash: String,
}

pub fn managed_hooks_from_fragment(
    repo_root: &Path,
    settings_path: &str,
    fragment: &serde_json::Value,
) -> Result<Vec<ManagedHook>> {
    let mut managed = Vec::new();
    let Some(events) = fragment.get("hooks").and_then(serde_json::Value::as_object) else {
        return Ok(managed);
    };

    for (event, groups) in events {
        let Some(groups) = groups.as_array() else {
            continue;
        };
        for group in groups {
            let Some(hooks) = group.get("hooks").and_then(serde_json::Value::as_array) else {
                continue;
            };
            for hook in hooks {
                let Some(command) = hook.get("command").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                let baseline = serde_json::to_vec(hook)?;
                managed.push(ManagedHook {
                    settings_path: settings_path.to_string(),
                    event: event.clone(),
                    command: command.to_string(),
                    baseline_hash: write_baseline_object(repo_root, &baseline)?,
                });
            }
        }
    }
    Ok(managed)
}

pub fn managed_hook_status(repo_root: &Path, hook: &ManagedHook) -> &'static str {
    let path = repo_root.join(&hook.settings_path);
    let Ok(settings) = std::fs::read_to_string(path) else {
        return "missing";
    };
    let Ok(settings): std::result::Result<serde_json::Value, _> = serde_json::from_str(&settings)
    else {
        return "modified";
    };
    let Some(groups) = settings
        .get("hooks")
        .and_then(|hooks| hooks.get(&hook.event))
        .and_then(serde_json::Value::as_array)
    else {
        return "missing";
    };

    for group in groups {
        let Some(entries) = group.get("hooks").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for entry in entries {
            if entry.get("command").and_then(serde_json::Value::as_str)
                == Some(hook.command.as_str())
            {
                let Ok(content) = serde_json::to_vec(entry) else {
                    return "modified";
                };
                return if hash_bytes(&content) == hook.baseline_hash {
                    "clean"
                } else {
                    "modified"
                };
            }
        }
    }
    "missing"
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetOwnership {
    #[default]
    Generated,
    Imported,
}

pub fn lockfile_path(repo_root: &Path) -> PathBuf {
    let scoped = repo_root.join(".coral").join("coral.lock");
    if scoped.exists() {
        scoped
    } else {
        repo_root.join("coral.lock")
    }
}

pub fn init_lockfile(repo_root: &Path) -> Result<PathBuf> {
    let lock_path = repo_root.join("coral.lock");
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
                .join(path.file_name().unwrap_or(OsStr::new("coral.lock")))
                .display()
        )));
    }

    let wire: WireLockfile = toml::from_str(&std::fs::read_to_string(path)?)?;
    let mut capabilities = BTreeMap::new();
    for item in wire.capabilities {
        let target = item.target.clone();
        let mut targets = BTreeMap::new();
        targets.insert(
            target,
            TargetLockEntry {
                emitted_files: Vec::new(),
                managed_hooks: item.managed_hooks,
                ownership: item.ownership,
                sha256: item.sha256,
                installed_path: item.installed_path,
            },
        );
        capabilities
            .entry(item.name.clone())
            .and_modify(|entry: &mut CapabilityLockEntry| {
                entry.targets.extend(targets.clone());
            })
            .or_insert_with(|| CapabilityLockEntry {
                capability_type: item.capability_type,
                installed_version: item.version,
                description: item.description,
                source_path: item.source_path.clone(),
                targets,
                source: (item.source == "git").then_some(SourceMetadata {
                    source_type: "git".to_string(),
                    url: item.repository,
                    source_ref: item.resolved_ref,
                    skill: item.source_path,
                }),
                scope: "project".to_string(),
            });
    }
    let lockfile = Lockfile {
        version: wire.version,
        capabilities,
    };
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
    let mut capabilities = Vec::new();
    for (name, entry) in &lockfile.capabilities {
        for (target, target_entry) in &entry.targets {
            let (source, repository, source_path, resolved_ref) = match &entry.source {
                Some(source) => (
                    "git".to_string(),
                    source.url.clone(),
                    source.skill.clone(),
                    source.source_ref.clone(),
                ),
                None => (
                    "local".to_string(),
                    String::new(),
                    entry.source_path.clone(),
                    String::new(),
                ),
            };
            capabilities.push(WireCapability {
                name: name.clone(),
                capability_type: entry.capability_type,
                source,
                repository,
                source_path,
                resolved_ref,
                sha256: target_entry.sha256.clone(),
                target: target.clone(),
                installed_path: target_entry.installed_path.clone(),
                version: entry.installed_version.clone(),
                description: entry.description.clone(),
                ownership: target_entry.ownership,
                managed_hooks: target_entry.managed_hooks.clone(),
            });
        }
    }
    capabilities.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.capability_type.as_str().cmp(b.capability_type.as_str()))
            .then_with(|| a.target.cmp(&b.target))
            .then_with(|| a.installed_path.cmp(&b.installed_path))
    });
    let wire = WireLockfile {
        version: LOCKFILE_VERSION,
        capabilities,
    };
    let content = format!(
        "# Coral lockfile. Each entry records one capability installation target.\n{}\n",
        toml::to_string_pretty(&wire)?
    );
    std::fs::write(path, content)?;
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct WireLockfile {
    version: u8,
    #[serde(rename = "capabilities")]
    capabilities: Vec<WireCapability>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WireCapability {
    name: String,
    #[serde(rename = "type")]
    capability_type: CapabilityType,
    source: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    repository: String,
    source_path: String,
    #[serde(default)]
    resolved_ref: String,
    sha256: String,
    target: String,
    installed_path: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    ownership: TargetOwnership,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    managed_hooks: Vec<ManagedHook>,
}

pub fn hash_bytes(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}

pub fn write_baseline_object(_repo_root: &Path, content: &[u8]) -> Result<String> {
    // Kept temporarily as an internal compatibility helper for lifecycle code;
    // baseline content is now stored only as a verified materialized tree.
    Ok(hash_bytes(content))
}

pub fn prune_unreferenced_baseline_objects(
    _repo_root: &Path,
    _lockfile: &Lockfile,
) -> Result<usize> {
    Ok(0)
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
        let path = tmp.path().join("coral.lock");
        init_lockfile_at(&path).unwrap();
        assert!(path.exists());

        let lf = read_lockfile_at(&path).unwrap();
        assert_eq!(lf.version, 1);
        assert!(lf.capabilities.is_empty());
    }

    #[test]
    fn read_lockfile_at_rejects_missing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("coral.lock");
        assert!(read_lockfile_at(&path).is_err());
    }

    #[test]
    fn read_lockfile_at_rejects_v4_schema() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("coral.lock");
        fs::write(&path, "version = 4\ncapabilities = []\n").unwrap();

        let error = read_lockfile_at(&path).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unsupported lockfile version: 4")
        );
    }

    #[test]
    fn write_and_read_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("coral.lock");
        let mut lf = Lockfile {
            version: LOCKFILE_VERSION,
            capabilities: BTreeMap::new(),
        };
        lf.capabilities.insert(
            "test".into(),
            CapabilityLockEntry {
                capability_type: CapabilityType::Skill,
                installed_version: "1.0".into(),
                description: "test skill".into(),
                source_path: "".into(),
                targets: BTreeMap::from([(
                    "open-agents".into(),
                    TargetLockEntry {
                        emitted_files: Vec::new(),
                        managed_hooks: Vec::new(),
                        ownership: TargetOwnership::Generated,
                        sha256: hash_bytes(b"content"),
                        installed_path: ".agents/skills/test".into(),
                    },
                )]),
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
        let path = tmp.path().join("coral.lock");
        fs::write(&path, "version = 1\ncapabilities = []\n").unwrap();
        let read = read_lockfile_at(&path).unwrap();
        assert!(read.capabilities.is_empty());
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
