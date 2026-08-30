use std::fs;
use std::path::Path;

use crate::adapter::{AdapterKind, AgentAdapter, resolve_capability};
use crate::error::{Result, TuffError};
use crate::git;
use crate::lockfile;
use crate::manifest::{self, load_manifest};
use crate::resolver::{self, Scope};

use super::add::{SourceMetaInput, install_capability};
use super::{capability_index, home_dir, resolve_agent_selection};

fn update_local_baseline(
    scope_root: &Path,
    id: &str,
    target_ids: &[String],
    check: bool,
    force: bool,
) -> Result<()> {
    if force {
        return Err(TuffError::new(
            "--force is only valid for git-sourced capabilities; local updates accept the current files as the new baseline",
        ));
    }

    let lf = lockfile::require_lockfile(scope_root)?;
    let entry = lf
        .capabilities
        .get(id)
        .ok_or_else(|| TuffError::new(format!("'{}' is not installed", id)))?;

    let mut updates = Vec::new();
    let mut changed_files = 0usize;

    for target_id in target_ids {
        let target_entry = entry.targets.get(target_id).ok_or_else(|| {
            TuffError::new(format!(
                "'{}' is not installed for agent '{}'",
                id, target_id
            ))
        })?;

        if !target_entry.installed_path.is_empty() {
            let path = scope_root.join(&target_entry.installed_path);
            let current_hash = crate::cache::hash_tree(&path)?;
            if current_hash != target_entry.sha256 {
                changed_files += 1;
            }
            updates.push((target_id.clone(), current_hash));
            continue;
        }
        return Err(TuffError::new(format!(
            "lock entry for '{id}' and target '{target_id}' has no installed path"
        )));
    }

    if check {
        if changed_files == 0 {
            println!("'{}' is already up to date", id);
        } else {
            println!(
                "'{}' has {} local file(s) — update would record them as the new baseline",
                id, changed_files
            );
        }
        return Ok(());
    }

    let mut lf = lf;
    let installed = lf
        .capabilities
        .get_mut(id)
        .ok_or_else(|| TuffError::new(format!("'{}' is not installed", id)))?;
    for (target_id, update) in updates {
        let target_entry = installed.targets.get_mut(&target_id).ok_or_else(|| {
            TuffError::new(format!(
                "'{}' is not installed for agent '{}'",
                id, target_id
            ))
        })?;
        if !target_entry.installed_path.is_empty() {
            target_entry.sha256 = update.clone();
            crate::cache::populate(
                &super::home_dir()?,
                &update,
                &scope_root.join(&target_entry.installed_path),
            )?;
        }
    }
    lockfile::write_lockfile(scope_root, &lf)?;
    lockfile::prune_unreferenced_baseline_objects(scope_root, &lf)?;
    capability_index::regenerate_capability_index(scope_root)?;

    if changed_files == 0 {
        println!("'{}' is already up to date", id);
    } else {
        println!(
            "updated local baseline for '{}' ({} file(s))",
            id, changed_files
        );
    }
    Ok(())
}

fn is_in_place_source_path(source_path: &str) -> bool {
    source_path.is_empty()
        || source_path.starts_with(".agents/")
        || source_path == ".agents"
        || source_path.starts_with(".claude/")
        || source_path == ".claude"
}

fn update_local_from_source(
    scope_root: &Path,
    scope: Scope,
    id: &str,
    entry: &lockfile::CapabilityLockEntry,
    target_ids: &[String],
    check: bool,
    force: bool,
) -> Result<()> {
    let source_dir = lockfile::absolutize(scope_root, Path::new(&entry.source_path));
    let manifest = load_manifest(&source_dir)?;
    let capability = resolve_capability(&manifest)?;
    if capability.id != id {
        return Err(TuffError::new(format!(
            "sourcePath for '{}' points at capability '{}'",
            id, capability.id
        )));
    }

    let mut adapters = Vec::new();
    for tid in target_ids {
        let adapter = AdapterKind::from_id(tid).ok_or_else(|| {
            TuffError::new(format!(
                "unknown agent '{}'; run 'tuff agent list' to see available agents",
                tid
            ))
        })?;
        adapters.push(adapter);
    }

    let mut changed_files = 0usize;
    let mut has_local_drift = false;
    for adapter in &adapters {
        let planned_files = adapter.plan(&capability, scope_root)?;
        let target_entry = entry.targets.get(adapter.id()).ok_or_else(|| {
            TuffError::new(format!(
                "'{}' is not installed for agent '{}'",
                id,
                adapter.id()
            ))
        })?;
        for planned in &planned_files {
            let current_path = scope_root.join(&planned.path);
            let current = if current_path.exists() {
                fs::read(&current_path)?
            } else {
                Vec::new()
            };
            if current != planned.content {
                changed_files += 1;
            }
        }
        has_local_drift |= target_entry
            .emitted_files
            .iter()
            .any(|emitted| lockfile::drift_status(scope_root, emitted) != "clean");
    }

    if check {
        if has_local_drift && !force {
            println!(
                "'{}' has local changes — update would require --force to reload from local source",
                id
            );
        } else if changed_files == 0 {
            println!("'{}' is already up to date", id);
        } else {
            println!(
                "'{}' has {} source file(s) to apply from {}",
                id, changed_files, entry.source_path
            );
        }
        return Ok(());
    }

    if has_local_drift && !force {
        return Err(TuffError::new(format!(
            "'{}' has local changes; run 'tuff diff {}' first or use --force to reload from source",
            id, id
        )));
    }

    install_capability(
        scope_root,
        scope,
        &capability,
        &manifest,
        target_ids,
        None,
        true,
    )?;
    Ok(())
}

/// Re-resolve a catalog-installed MCP server against the catalog compiled
/// into this binary. The catalog version is the "upstream ref": a newer Tuff
/// can carry a newer catalog, and that is the only way an entry changes.
fn update_from_catalog(
    scope_root: &Path,
    scope: Scope,
    id: &str,
    entry: &lockfile::CapabilityLockEntry,
    target_ids: &[String],
    check: bool,
    force: bool,
) -> Result<()> {
    let source = entry
        .source
        .as_ref()
        .expect("catalog source checked by caller");
    let latest = crate::catalog::version();
    let Some(manifest) = crate::catalog::lookup(&source.skill)? else {
        return Err(TuffError::new(format!(
            "'{}' is no longer in the built-in catalog (installed from catalog {}); \
             delete it or reinstall from a path",
            source.skill, entry.installed_version
        )));
    };

    if latest == entry.installed_version {
        println!("'{}' is already up to date (catalog {latest})", id);
        return Ok(());
    }

    let mut all_clean = true;
    for target_id in target_ids {
        let target_entry = entry.targets.get(target_id).ok_or_else(|| {
            TuffError::new(format!(
                "'{}' is not installed for agent '{}'",
                id, target_id
            ))
        })?;
        if crate::cache::hash_tree(&scope_root.join(&target_entry.installed_path))?
            != target_entry.sha256
        {
            all_clean = false;
        }
    }

    if check {
        if all_clean {
            println!(
                "'{}' can be updated cleanly: catalog {} → {latest}",
                id, entry.installed_version
            );
        } else {
            println!(
                "'{}' has local changes — update would replace the materialized tree",
                id
            );
        }
        return Ok(());
    }

    if !all_clean && !force {
        return Err(TuffError::new(format!(
            "'{id}' has local changes; run 'tuff diff {id}' first or use --force to reload from the catalog"
        )));
    }

    let capability = resolve_capability(&manifest)?;
    install_capability(
        scope_root,
        scope,
        &capability,
        &manifest,
        target_ids,
        Some(&SourceMetaInput {
            source_type: crate::catalog::SOURCE_TYPE.to_string(),
            url: crate::catalog::SOURCE_URL.to_string(),
            source_ref: latest,
            skill: source.skill.clone(),
        }),
        true,
    )
}

pub fn cmd_update(
    repo_root: &Path,
    id: &str,
    scope_str: Option<&str>,
    requested_targets: &[String],
    check: bool,
    force: bool,
) -> Result<()> {
    let (scope, entry, scope_root) = if let Some(s) = scope_str {
        let scope = resolver::Scope::parse(s)
            .ok_or_else(|| TuffError::new(format!("invalid scope '{}'", s)))?;
        let root = match scope {
            Scope::Project => repo_root.to_path_buf(),
            Scope::Global => home_dir()?,
        };
        let lf = lockfile::require_lockfile(&root)?;
        let entry = lf.capabilities.get(id).ok_or_else(|| {
            TuffError::new(format!(
                "'{}' is not installed in {} scope",
                id,
                scope.as_str()
            ))
        })?;
        (scope, entry.clone(), root)
    } else {
        match resolver::resolve_entry(id, repo_root)? {
            Some((s, e)) => {
                let root = match s {
                    Scope::Project => repo_root.to_path_buf(),
                    Scope::Global => home_dir()?,
                };
                (s, e, root)
            }
            None => {
                return Err(TuffError::new(format!("'{}' is not installed", id)));
            }
        }
    };

    let target_ids =
        resolve_agent_selection(&scope_root, requested_targets, scope == Scope::Global)?;

    if entry.source.is_none() && is_in_place_source_path(&entry.source_path) {
        return update_local_baseline(&scope_root, id, &target_ids, check, force);
    }

    if entry.source.is_none() {
        return update_local_from_source(&scope_root, scope, id, &entry, &target_ids, check, force);
    }

    let source = entry.source.as_ref().expect("source checked above");

    if source.source_type == crate::catalog::SOURCE_TYPE {
        return update_from_catalog(&scope_root, scope, id, &entry, &target_ids, check, force);
    }

    let (_source_guard, cache_dir, _clean_url) = git::clone_to_temp(&source.url, None)?;
    let latest_sha = git::resolve_ref(&cache_dir)?;

    if latest_sha == entry.installed_version {
        println!("'{}' is already up to date", id);
        return Ok(());
    }

    let skill_dir = git::discover_capability(&cache_dir, &source.skill, entry.capability_type)?;

    if force {
        let manifest = manifest::synthetic_manifest(&skill_dir, id, &latest_sha)?;
        let capability = resolve_capability(&manifest)?;
        return install_capability(
            &scope_root,
            scope,
            &capability,
            &manifest,
            &target_ids,
            Some(&SourceMetaInput {
                source_type: "git".to_string(),
                url: source.url.clone(),
                source_ref: latest_sha,
                skill: source.skill.clone(),
            }),
            true,
        );
    }

    let mut all_clean = true;
    for target_id in &target_ids {
        let target_entry = entry.targets.get(target_id).ok_or_else(|| {
            TuffError::new(format!(
                "'{}' is not installed for agent '{}'",
                id, target_id
            ))
        })?;
        let current = scope_root.join(&target_entry.installed_path);
        if crate::cache::hash_tree(&current)? != target_entry.sha256 {
            all_clean = false;
        }
    }

    if check {
        if all_clean {
            println!("'{}' can be updated cleanly (no local changes)", id);
        } else if !all_clean {
            println!(
                "'{}' has local changes — update would replace the materialized tree",
                id
            );
        } else {
            println!("'{}' is up to date", id);
        }
        return Ok(());
    }

    if !all_clean {
        return Err(TuffError::new(format!(
            "'{id}' has local changes; run 'tuff diff {id}' first or use --force to reload from source"
        )));
    }

    let manifest = manifest::synthetic_manifest(&skill_dir, id, &latest_sha)?;
    let primitive = resolve_capability(&manifest)?;
    install_capability(
        &scope_root,
        scope,
        &primitive,
        &manifest,
        &target_ids,
        Some(&SourceMetaInput {
            source_type: "git".to_string(),
            url: source.url.clone(),
            source_ref: latest_sha,
            skill: source.skill.clone(),
        }),
        true,
    )
}
