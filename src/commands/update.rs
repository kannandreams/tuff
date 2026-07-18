use std::fs;
use std::path::Path;

use crate::adapter::{AdapterKind, resolve_capability, AgentAdapter};
use crate::error::{CoralError, Result};
use crate::git;
use crate::lockfile;
use crate::manifest::{self, load_manifest};
use crate::resolver::{self, Scope};

use super::add::{install_capability, SourceMetaInput};
use super::{capability_relative_path, home_dir, resolve_agent_selection};

fn update_local_baseline(
    scope_root: &Path,
    id: &str,
    target_ids: &[String],
    check: bool,
    force: bool,
) -> Result<()> {
    if force {
        return Err(CoralError::new(
            "--force is only valid for git-sourced capabilities; local updates accept the current files as the new baseline",
        ));
    }

    let lf = lockfile::require_lockfile(scope_root)?;
    let entry = lf
        .capabilities
        .get(id)
        .ok_or_else(|| CoralError::new(format!("'{}' is not installed", id)))?;

    let mut updates = Vec::new();
    let mut changed_files = 0usize;

    for target_id in target_ids {
        let target_entry = entry.targets.get(target_id).ok_or_else(|| {
            CoralError::new(format!(
                "'{}' is not installed for agent '{}'",
                id, target_id
            ))
        })?;

        for emitted in &target_entry.emitted_files {
            let local_path = scope_root.join(&emitted.path);
            if !local_path.is_file() {
                return Err(CoralError::new(format!(
                    "tracked file is missing for '{}': {}",
                    id,
                    local_path.display()
                )));
            }

            let content = fs::read(&local_path)?;
            let baseline =
                lockfile::read_baseline_object(scope_root, &emitted.baseline_hash)?;
            if content != baseline {
                changed_files += 1;
            }
            updates.push((target_id.clone(), emitted.path.clone(), content));
        }
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
        .ok_or_else(|| CoralError::new(format!("'{}' is not installed", id)))?;
    for (target_id, path, content) in updates {
        let target_entry = installed.targets.get_mut(&target_id).ok_or_else(|| {
            CoralError::new(format!(
                "'{}' is not installed for agent '{}'",
                id, target_id
            ))
        })?;
        let emitted = target_entry
            .emitted_files
            .iter_mut()
            .find(|emitted| emitted.path == path)
            .ok_or_else(|| CoralError::new(format!("tracked file disappeared: {}", path)))?;
        emitted.hash = lockfile::hash_bytes(&content);
        emitted.baseline_hash = lockfile::write_baseline_object(scope_root, &content)?;
    }
    lockfile::write_lockfile(scope_root, &lf)?;
    lockfile::prune_unreferenced_baseline_objects(scope_root, &lf)?;

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
        return Err(CoralError::new(format!(
            "sourcePath for '{}' points at capability '{}'",
            id, capability.id
        )));
    }

    let mut adapters = Vec::new();
    for tid in target_ids {
        let adapter = AdapterKind::from_id(tid).ok_or_else(|| {
            CoralError::new(format!(
                "unknown agent '{}'; run 'coral agent list' to see available agents",
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
            CoralError::new(format!(
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
        return Err(CoralError::new(format!(
            "'{}' has local changes; run 'coral diff {}' first or use --force to reload from source",
            id, id
        )));
    }

    install_capability(scope_root, scope, &capability, &manifest, target_ids, None)?;
    Ok(())
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
        let scope = resolver::Scope::from_str(s)
            .ok_or_else(|| CoralError::new(format!("invalid scope '{}'", s)))?;
        let root = match scope {
            Scope::Project => repo_root.to_path_buf(),
            Scope::Global => home_dir()?,
        };
        let lf = lockfile::require_lockfile(&root)?;
        let entry = lf.capabilities.get(id).ok_or_else(|| {
            CoralError::new(format!(
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
                return Err(CoralError::new(format!("'{}' is not installed", id)));
            }
        }
    };

    let target_ids = resolve_agent_selection(&scope_root, requested_targets)?;

    if entry.source.is_none() && is_in_place_source_path(&entry.source_path) {
        return update_local_baseline(&scope_root, id, &target_ids, check, force);
    }

    if entry.source.is_none() {
        return update_local_from_source(
            &scope_root, scope, id, &entry, &target_ids, check, force,
        );
    }

    let source = entry.source.as_ref().expect("source checked above");

    let (cache_dir, _clean_url) = git::clone_or_fetch(&source.url)?;
    let latest_sha = git::resolve_ref(&cache_dir)?;

    if latest_sha == entry.installed_version {
        println!("'{}' is already up to date", id);
        return Ok(());
    }

    let skill_dir = git::discover_skill(&cache_dir, &source.skill)?;

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
                source_ref: latest_sha.clone(),
                skill: source.skill.clone(),
            }),
        );
    }

    let mut all_clean = true;
    let mut had_conflicts = false;
    let mut would_overwrite = false;

    for target_id in &target_ids {
        let target_entry = entry.targets.get(target_id).ok_or_else(|| {
            CoralError::new(format!(
                "'{}' is not installed for agent '{}'",
                id, target_id
            ))
        })?;
        for emitted in &target_entry.emitted_files {
            let rel_path =
                capability_relative_path(&emitted.path, &source.skill);

            let local_path = scope_root.join(&emitted.path);
            let upstream_path = skill_dir.join(&rel_path);

            let local_content =
                std::fs::read_to_string(&local_path).unwrap_or_default();
            let baseline_content = String::from_utf8(
                lockfile::read_baseline_object(
                    &scope_root,
                    &emitted.baseline_hash,
                )?,
            )
            .map_err(|error| {
                CoralError::new(format!(
                    "baseline object is not valid UTF-8 for '{}': {}",
                    emitted.path, error
                ))
            })?;
            let upstream_content =
                std::fs::read_to_string(&upstream_path).unwrap_or_default();

            if local_content == upstream_content {
                continue;
            }

            let local_status = lockfile::drift_status(&scope_root, emitted);
            if local_status != "clean" || local_content != baseline_content {
                all_clean = false;
                would_overwrite = true;
            }

            if !check {
                let report = crate::diff::merge_with_baseline_content(
                    &baseline_content,
                    &local_path,
                    &upstream_path,
                )?;

                if let Some(reports) = report {
                    had_conflicts = true;
                    for r in reports {
                        for c in &r.conflicts {
                            eprintln!("  ✗ {}: {}", r.file_path, c.description);
                            eprintln!(
                                "    <<<<<< local\n{}\n    ======",
                                c.local.trim()
                            );
                            eprintln!(
                                "    {}\n    >>>>>> upstream",
                                c.upstream.trim()
                            );
                        }
                    }
                    eprintln!(
                        "\n  To write conflict markers: coral update {} --write-conflicts",
                        id
                    );
                }
            }
        }
    }

    if check {
        if all_clean {
            println!("'{}' can be updated cleanly (no local changes)", id);
        } else if would_overwrite {
            println!(
                "'{}' has local changes — update would attempt three-way merge",
                id
            );
        } else {
            println!("'{}' is up to date", id);
        }
        return Ok(());
    }

    if had_conflicts {
        return Err(CoralError::new(
            "conflicts found — local files have not been modified. Resolve conflicts manually or use --force to overwrite.",
        ));
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
            source_ref: latest_sha.clone(),
            skill: source.skill.clone(),
        }),
    )
}
