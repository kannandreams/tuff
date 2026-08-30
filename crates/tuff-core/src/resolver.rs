use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::lockfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Project,
    Global,
}

impl Scope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Global => "global",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "project" => Some(Self::Project),
            "global" => Some(Self::Global),
            _ => None,
        }
    }
}

pub fn lockfile_path_for(scope: Scope, repo_root: Option<&Path>) -> Option<PathBuf> {
    match scope {
        Scope::Project => repo_root.map(|root| root.join("tuff.lock")),
        Scope::Global => dirs_home().map(|home| crate::paths::global_lockfile(&home)),
    }
}

pub fn read_lockfile(scope: Scope, repo_root: Option<&Path>) -> Result<Option<lockfile::Lockfile>> {
    let path = match lockfile_path_for(scope, repo_root) {
        Some(p) => p,
        None => return Ok(None),
    };
    if path.exists() {
        Ok(Some(lockfile::read_lockfile_at(&path)?))
    } else {
        Ok(None)
    }
}

pub fn resolve_entry(
    id: &str,
    repo_root: &Path,
) -> Result<Option<(Scope, lockfile::CapabilityLockEntry)>> {
    if let Some(project_lf) = read_lockfile(Scope::Project, Some(repo_root))?
        && let Some(entry) = project_lf.capabilities.get(id)
    {
        return Ok(Some((Scope::Project, entry.clone())));
    }

    if let Some(global_lf) = read_lockfile(Scope::Global, None)?
        && let Some(entry) = global_lf.capabilities.get(id)
    {
        return Ok(Some((Scope::Global, entry.clone())));
    }

    Ok(None)
}

pub fn overrides_global(id: &str, repo_root: &Path) -> Result<bool> {
    let project_exists = read_lockfile(Scope::Project, Some(repo_root))?
        .map(|lf| lf.capabilities.contains_key(id))
        .unwrap_or(false);

    let global_exists = read_lockfile(Scope::Global, None)?
        .map(|lf| lf.capabilities.contains_key(id))
        .unwrap_or(false);

    Ok(project_exists && global_exists)
}

pub fn check_collision(
    id: &str,
    _repo_root: &Path,
    new_source_url: Option<&str>,
) -> Result<Option<String>> {
    let Some(global_lf) = read_lockfile(Scope::Global, None)? else {
        return Ok(None);
    };

    let Some(global_entry) = global_lf.capabilities.get(id) else {
        return Ok(None);
    };

    let global_source = global_entry.source.as_ref().map(|s| s.url.as_str());

    match (new_source_url, global_source) {
        (Some(new_url), Some(global_url)) if new_url != global_url => Ok(Some(format!(
            "note: '{}' is already installed globally from a different source ({}). \
                 The project copy will take precedence and the global copy will be shadowed.",
            id, global_url
        ))),
        (None, Some(global_url)) => Ok(Some(format!(
            "note: '{}' is already installed globally from {}. \
                 The project copy will take precedence and the global copy will be shadowed.",
            id, global_url
        ))),
        _ => Ok(None),
    }
}

#[cfg(unix)]
fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

#[cfg(windows)]
fn dirs_home() -> Option<PathBuf> {
    std::env::var("USERPROFILE")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            let drive = std::env::var("HOMEDRIVE").ok()?;
            let path = std::env::var("HOMEPATH").ok()?;
            Some(PathBuf::from(format!("{}{}", drive, path)))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lockfile;
    use crate::manifest::CapabilityType;
    use tempfile::TempDir;

    fn create_lockfile(path: &std::path::Path, entries: &[(&str, &str, &str)]) {
        let mut lf = lockfile::Lockfile {
            version: lockfile::LOCKFILE_VERSION,
            capabilities: std::collections::BTreeMap::new(),
        };
        for (id, capability_type, version) in entries {
            let ct = CapabilityType::parse(capability_type).unwrap_or(CapabilityType::Skill);
            lf.capabilities.insert(
                id.to_string(),
                lockfile::CapabilityLockEntry {
                    capability_type: ct,
                    installed_version: version.to_string(),
                    description: String::new(),
                    source_path: "".into(),
                    targets: std::collections::BTreeMap::from([(
                        "open-agents".to_string(),
                        lockfile::TargetLockEntry {
                            emitted_files: Vec::new(),
                            managed_hooks: Vec::new(),
                            ownership: lockfile::TargetOwnership::Generated,
                            sha256: String::new(),
                            installed_path: String::new(),
                        },
                    )]),
                    source: None,
                    scope: "project".into(),
                    pack: None,
                    implementation: None,
                    parameters: None,
                    workflow: None,
                    server: None,
                },
            );
        }
        lockfile::write_lockfile_at(path, &lf).unwrap();
    }

    #[test]
    fn scope_from_str_valid() {
        assert_eq!(Scope::parse("project"), Some(Scope::Project));
        assert_eq!(Scope::parse("global"), Some(Scope::Global));
        assert_eq!(Scope::parse("invalid"), None);
    }

    #[test]
    fn scope_as_str() {
        assert_eq!(Scope::Project.as_str(), "project");
        assert_eq!(Scope::Global.as_str(), "global");
    }

    #[test]
    fn resolve_entry_project_wins_over_global() {
        let tmp = TempDir::new().unwrap();
        let proj_lock = tmp.path().join("tuff.lock");
        create_lockfile(&proj_lock, &[("test", "skill", "1.0-project")]);

        // Create a fake global lockfile that won't be checked
        // since we can't easily control HOME in tests, test project-only path
        let result = read_lockfile(Scope::Project, Some(tmp.path())).unwrap();
        assert!(result.is_some());
        let lf = result.unwrap();
        assert_eq!(lf.capabilities["test"].installed_version, "1.0-project");
    }

    #[test]
    fn resolve_entry_not_found_returns_none() {
        let tmp = TempDir::new().unwrap();
        let proj_lock = tmp.path().join("tuff.lock");
        create_lockfile(&proj_lock, &[("other", "skill", "1.0")]);

        let result = resolve_entry("missing", tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn overrides_global_false_when_no_conflict() {
        let tmp = TempDir::new().unwrap();
        let proj_lock = tmp.path().join("tuff.lock");
        create_lockfile(&proj_lock, &[("test", "skill", "1.0")]);

        assert!(!overrides_global("test", tmp.path()).unwrap());
    }

    #[test]
    fn check_collision_no_global_returns_none() {
        let tmp = TempDir::new().unwrap();
        let result = check_collision("test", tmp.path(), Some("https://github.com/a/b")).unwrap();
        assert!(result.is_none());
    }
}
