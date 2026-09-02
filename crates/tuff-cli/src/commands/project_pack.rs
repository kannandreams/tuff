use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::error::{Result, TuffError};
use crate::git;
use crate::lockfile::{self, CapabilityLockEntry, Lockfile};
use crate::manifest::{self, CapabilityManifest, CapabilityType};
use crate::pack::{self, LoadedPack, PackManifest, PackMember};

pub(crate) const TUFF_CLI_GUIDE_ID: &str = "tuff-cli-guide";

pub(crate) struct PreparedProjectPack {
    pub(crate) loaded: LoadedPack,
    pub(crate) lock: Lockfile,
    _staging: TempDir,
}

pub(crate) fn default_project_capabilities(lock: &Lockfile) -> Vec<String> {
    lock.capabilities
        .iter()
        .filter(|(id, _entry)| {
            id.as_str() != TUFF_CLI_GUIDE_ID
                && id.as_str() != super::capability_index::CAPABILITY_INDEX_ID
        })
        .map(|(id, _)| id.clone())
        .collect()
}

pub(crate) fn prepare_project_pack(
    repo_root: &Path,
    mut source_manifest: PackManifest,
) -> Result<PreparedProjectPack> {
    let project = source_manifest.project.take().ok_or_else(|| {
        TuffError::usage("project pack manifest is missing a [project] selection")
    })?;
    let lock = lockfile::require_lockfile(repo_root)?;
    let staging = tempfile::Builder::new()
        .prefix("tuff-project-pack-")
        .tempdir()?;
    let capabilities_root = staging.path().join("capabilities");
    fs::create_dir_all(&capabilities_root)?;

    let mut pending = project.capabilities.into_iter().collect::<BTreeSet<_>>();
    let mut selected = BTreeMap::<String, String>::new();
    while let Some(id) = pending.pop_first() {
        if selected.contains_key(&id) {
            continue;
        }
        let entry = lock.capabilities.get(&id).ok_or_else(|| {
            TuffError::not_found(format!("capability '{id}' is not tracked in this project"))
                .with_hint("run 'tuff list --scope project' to see available capabilities")
        })?;
        let relative = format!("capabilities/member-{:04}", selected.len() + 1);
        let destination = staging.path().join(&relative);
        let capability = materialize_capability(repo_root, &id, entry, &destination)?;
        validate_source_identity(&id, entry, &capability)?;

        if let Some(workflow) = &capability.workflow {
            for requirement in &workflow.requires {
                let required = lock.capabilities.get(&requirement.id).ok_or_else(|| {
                    TuffError::usage(format!(
                        "workflow '{}' requires '{}', but it is not tracked in this project",
                        capability.id, requirement.id
                    ))
                })?;
                if required.capability_type != requirement.capability_type {
                    return Err(TuffError::usage(format!(
                        "workflow '{}' requires '{}' as {}, but tuff.lock records it as {}",
                        capability.id,
                        requirement.id,
                        requirement.capability_type,
                        required.capability_type
                    )));
                }
                pending.insert(requirement.id.clone());
            }
        }
        selected.insert(id, relative);
    }

    validate_clean_installations(repo_root, &lock, selected.keys())?;
    source_manifest.capabilities = selected
        .values()
        .map(|path| PackMember { path: path.clone() })
        .collect();
    pack::write_manifest(
        &staging.path().join(pack::PACK_MANIFEST_FILE),
        &source_manifest,
    )?;
    let loaded = pack::load_pack(staging.path())?;

    Ok(PreparedProjectPack {
        loaded,
        lock,
        _staging: staging,
    })
}

fn materialize_capability(
    repo_root: &Path,
    id: &str,
    entry: &CapabilityLockEntry,
    destination: &Path,
) -> Result<CapabilityManifest> {
    let capability = match &entry.source {
        lockfile::CapabilitySource::Git(git) => {
            if entry.capability_type != CapabilityType::Skill {
                return Err(unsupported_source_error(id, entry));
            }
            let (_guard, checkout, _) = git::clone_to_temp(&git.url, Some(git.git_ref.as_str()))?;
            let source_dir = git::discover_capability(&checkout, &git.path, entry.capability_type)?;
            let mut capability = manifest::synthetic_manifest(&source_dir, id, &entry.version)?;
            capability.description.clone_from(&entry.description);
            capability
        }
        lockfile::CapabilitySource::Local(local) => {
            let source_dir = lockfile::absolutize(repo_root, Path::new(&local.path));
            if !local.path.is_empty() && source_dir.join("tuff.toml").is_file() {
                manifest::load_manifest(&source_dir)?
            } else if entry.capability_type == CapabilityType::Skill {
                let installed = installed_source(repo_root, id, entry)?;
                let mut capability = manifest::synthetic_manifest(&installed, id, &entry.version)?;
                capability.description.clone_from(&entry.description);
                capability
            } else {
                return Err(unsupported_source_error(id, entry));
            }
        }
        lockfile::CapabilitySource::Pack(_) | lockfile::CapabilitySource::Catalog(_) => {
            if entry.capability_type == CapabilityType::Skill {
                let installed = installed_source(repo_root, id, entry)?;
                let mut capability = manifest::synthetic_manifest(&installed, id, &entry.version)?;
                capability.description.clone_from(&entry.description);
                capability
            } else {
                return Err(unsupported_source_error(id, entry));
            }
        }
    };

    copy_manifest_source(&capability, destination)?;
    Ok(capability)
}

fn installed_source(repo_root: &Path, id: &str, entry: &CapabilityLockEntry) -> Result<PathBuf> {
    let path = entry
        .targets
        .values()
        .find(|target| !target.installed_path.is_empty())
        .map(|target| repo_root.join(&target.installed_path))
        .ok_or_else(|| unsupported_source_error(id, entry))?;
    if !path.is_dir() {
        return Err(TuffError::not_found(format!(
            "cannot package '{id}': installed capability directory is missing at {}",
            path.display()
        ))
        .with_hint(format!("run 'tuff update {id}', or reinstall it")));
    }
    Ok(path)
}

fn copy_manifest_source(capability: &CapabilityManifest, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    manifest::write_manifest(&destination.join("tuff.toml"), capability)?;
    for source in capability.source_files()? {
        let relative = source.strip_prefix(&capability.root).map_err(|_| {
            TuffError::refused(format!(
                "capability source escaped its root: {}",
                source.display()
            ))
        })?;
        crate::tool::check_path_traversal(&relative.to_string_lossy())?;
        let metadata = fs::symlink_metadata(&source)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(TuffError::refused(format!(
                "capability source must be a regular file: {}",
                source.display()
            )));
        }
        let output = destination.join(relative);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, output)?;
    }
    Ok(())
}

fn validate_source_identity(
    expected_id: &str,
    entry: &CapabilityLockEntry,
    capability: &CapabilityManifest,
) -> Result<()> {
    if capability.id != expected_id
        || capability.capability_type != entry.capability_type
        || capability.version != entry.version
        || capability.description != entry.description
    {
        return Err(TuffError::drift(format!(
            "source for '{expected_id}' no longer matches its accepted tuff.lock metadata"
        ))
        .with_hint(format!(
            "run 'tuff update {expected_id}' before building the pack"
        )));
    }
    Ok(())
}

fn validate_clean_installations<'a>(
    repo_root: &Path,
    lock: &Lockfile,
    ids: impl Iterator<Item = &'a String>,
) -> Result<()> {
    let mut failures = Vec::new();
    for id in ids {
        let entry = &lock.capabilities[id];
        for (target_id, target) in &entry.targets {
            let mut changed = Vec::new();
            if target.installed_path.is_empty()
                || crate::cache::hash_tree(&repo_root.join(&target.installed_path))
                    .map_or(true, |hash| hash != target.sha256)
            {
                changed.push(target.installed_path.clone());
            }
            for hook in &target.managed_hooks {
                if lockfile::managed_hook_status(repo_root, hook) != "clean" {
                    changed.push(format!("{}#{}", hook.settings_path, hook.event));
                }
            }
            if !changed.is_empty() {
                failures.push(format!("{id} ({target_id}): {}", changed.join(", ")));
            }
        }
    }
    if failures.is_empty() {
        return Ok(());
    }
    Err(TuffError::drift(format!(
        "selected capabilities have unaccepted changes:\n  - {}",
        failures.join("\n  - ")
    ))
    .with_hint("run 'tuff update <capability>' for intentional changes, then build again"))
}

fn unsupported_source_error(id: &str, entry: &CapabilityLockEntry) -> TuffError {
    let provenance = entry
        .source
        .as_pack()
        .map(|pack| format!(" from pack {} {}", pack.name, pack.version))
        .unwrap_or_default();
    TuffError::unsupported(format!(
        "cannot reconstruct portable source for {} capability '{id}'{provenance}",
        entry.capability_type
    ))
    .with_hint("reinstall it from a manifest-backed local source, or select a different capability")
}
