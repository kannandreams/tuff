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

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "project" => Some(Self::Project),
            "global" => Some(Self::Global),
            _ => None,
        }
    }
}

pub fn home_coral_dir() -> Option<PathBuf> {
    dirs_home().map(|h| h.join(".coral"))
}

pub fn coral_dir(scope: Scope, repo_root: Option<&Path>) -> Option<PathBuf> {
    match scope {
        Scope::Project => repo_root.map(|r| r.join(".coral")),
        Scope::Global => home_coral_dir(),
    }
}

pub fn lockfile_path_for(scope: Scope, repo_root: Option<&Path>) -> Option<PathBuf> {
    coral_dir(scope, repo_root).map(|d| d.join("coral-lock.json"))
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
    if let Some(project_lf) = read_lockfile(Scope::Project, Some(repo_root))? {
        if let Some(entry) = project_lf.capabilities.get(id) {
            return Ok(Some((Scope::Project, entry.clone())));
        }
    }

    if let Some(global_lf) = read_lockfile(Scope::Global, None)? {
        if let Some(entry) = global_lf.capabilities.get(id) {
            return Ok(Some((Scope::Global, entry.clone())));
        }
    }

    Ok(None)
}

pub fn overrides_global(
    id: &str,
    repo_root: &Path,
) -> Result<bool> {
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
        (Some(new_url), Some(global_url)) if new_url != global_url => {
            Ok(Some(format!(
                "note: '{}' is already installed globally from a different source ({}). \
                 The project copy will take precedence and the global copy will be shadowed.",
                id, global_url
            )))
        }
        (None, Some(global_url)) => {
            Ok(Some(format!(
                "note: '{}' is already installed globally from {}. \
                 The project copy will take precedence and the global copy will be shadowed.",
                id, global_url
            )))
        }
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
