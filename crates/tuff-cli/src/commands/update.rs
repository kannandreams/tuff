use std::fs;
use std::path::Path;

use crate::adapter::{AdapterKind, AgentAdapter, resolve_capability};
use crate::error::{Result, TuffError};
use crate::git;
use crate::lockfile;
use crate::manifest::{self, load_manifest};
use crate::oci::OciTransferOptions;
use crate::resolver::{self, Scope};

use super::add::install_capability;
use super::{capability_index, home_dir, resolve_agent_selection};
use crate::lockfile::{CapabilitySource, CatalogSource, GitSource};

fn update_local_baseline(
    scope_root: &Path,
    scope: Scope,
    id: &str,
    target_ids: &[String],
    check: bool,
    force: bool,
) -> Result<()> {
    if force {
        return Err(
            TuffError::usage("--force is only valid for git-sourced capabilities")
                .with_hint("local updates accept the current files as the new baseline"),
        );
    }

    let lf = lockfile::require_scoped_lockfile(scope_root, scope)?;
    let entry = lf
        .capabilities
        .get(id)
        .ok_or_else(|| TuffError::not_found(format!("'{}' is not installed", id)))?;

    let mut updates = Vec::new();
    let mut changed_files = 0usize;

    for target_id in target_ids {
        let target_entry = entry.targets.get(target_id).ok_or_else(|| {
            TuffError::not_found(format!(
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
        return Err(TuffError::corrupt(format!(
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
        .ok_or_else(|| TuffError::not_found(format!("'{}' is not installed", id)))?;
    for (target_id, update) in updates {
        let target_entry = installed.targets.get_mut(&target_id).ok_or_else(|| {
            TuffError::not_found(format!(
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
    lockfile::write_scoped_lockfile(scope_root, scope, &lf)?;
    capability_index::regenerate_capability_index(scope_root, scope)?;

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
    let source_path = entry.source.local_path().unwrap_or_default();
    let source_dir = lockfile::absolutize(scope_root, Path::new(source_path));
    let manifest = load_manifest(&source_dir)?;
    let capability = resolve_capability(&manifest)?;
    if capability.id != id {
        return Err(TuffError::usage(format!(
            "sourcePath for '{}' points at capability '{}'",
            id, capability.id
        )));
    }

    let mut adapters = Vec::new();
    for tid in target_ids {
        let adapter = AdapterKind::from_id(tid).ok_or_else(|| {
            TuffError::usage(format!("unknown agent '{}'", tid,))
                .with_hint("run 'tuff agent list' to see available agents")
        })?;
        adapters.push(adapter);
    }

    let mut changed_files = 0usize;
    let mut has_local_drift = false;
    for adapter in &adapters {
        let planned_files = adapter.plan(&capability, scope_root)?;
        let target_entry = entry.targets.get(adapter.id()).ok_or_else(|| {
            TuffError::not_found(format!(
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
        has_local_drift |= !target_entry.installed_path.is_empty()
            && crate::cache::hash_tree(&scope_root.join(&target_entry.installed_path))
                .map(|hash| hash != target_entry.sha256)
                .unwrap_or(true);
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
                id, changed_files, source_path
            );
        }
        return Ok(());
    }

    if has_local_drift && !force {
        return Err(
            TuffError::drift(format!("'{id}' has local changes")).with_hint(format!(
                "run 'tuff diff {id}' first, or use --force to reload from source"
            )),
        );
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

/// Re-resolve an MCP server that was installed from a registry.
///
/// The registry is the upstream here, so unlike a built-in catalog entry the
/// answer can change without Tuff being upgraded. The comparison is the
/// entry's published version; the drift rules are the catalog's, because the
/// installed artifact is the same shape either way.
#[allow(clippy::too_many_arguments)]
fn update_from_registry(
    scope_root: &Path,
    scope: Scope,
    id: &str,
    entry: &lockfile::CapabilityLockEntry,
    registry: &str,
    source: &CatalogSource,
    target_ids: &[String],
    check: bool,
    force: bool,
) -> Result<()> {
    let Some(server) = super::block_on_oci(crate::registry::fetch(registry, &source.id))? else {
        return Err(TuffError::not_found(format!(
            "'{}' is no longer published in {registry} (installed at {})",
            source.id, entry.version
        ))
        .with_hint("delete it, or reinstall it from a path"));
    };
    let manifest = crate::registry::to_manifest(&server, id)?;
    let latest = manifest.version.clone();

    let entry_drifted = target_ids.iter().any(|target_id| {
        entry
            .targets
            .get(target_id)
            .and_then(|target| target.managed_mcp_entry.as_ref())
            .is_some_and(|managed| {
                lockfile::managed_mcp_entry_status(scope_root, id, managed) != "clean"
            })
    });
    if latest == entry.version && !entry_drifted {
        println!("'{id}' is already up to date ({registry} {latest})");
        return Ok(());
    }
    if check {
        if entry_drifted {
            println!(
                "'{id}' has a hand-edited MCP config entry; update --force would restore the canonical entry"
            );
        } else {
            println!("'{id}' can be updated: {} → {latest}", entry.version);
        }
        return Ok(());
    }
    if entry_drifted && !force {
        return Err(
            TuffError::drift(format!("'{id}' has local changes")).with_hint(format!(
                "run 'tuff diff {id}' first, or use --force to reload from {registry}"
            )),
        );
    }
    let capability = resolve_capability(&manifest)?;
    install_capability(
        scope_root,
        scope,
        &capability,
        &manifest,
        target_ids,
        Some(CapabilitySource::Catalog(CatalogSource {
            id: source.id.clone(),
            version: latest,
            registry: Some(registry.to_string()),
        })),
        true,
    )
}

/// Re-resolve a catalog-installed MCP server against the catalog compiled
/// into this binary. Each entry's own version is the "upstream ref": a
/// newer Tuff can carry a newer version of that one entry, and that is the
/// only way it changes.
fn update_from_catalog(
    scope_root: &Path,
    scope: Scope,
    id: &str,
    entry: &lockfile::CapabilityLockEntry,
    target_ids: &[String],
    check: bool,
    force: bool,
) -> Result<()> {
    let CapabilitySource::Catalog(source) = &entry.source else {
        unreachable!("catalog source checked by caller");
    };
    if let Some(registry) = source.registry.as_deref() {
        return update_from_registry(
            scope_root, scope, id, entry, registry, source, target_ids, check, force,
        );
    }
    let Some(manifest) = crate::catalog::lookup(&source.id)? else {
        return Err(TuffError::not_found(format!(
            "'{}' is no longer in the built-in catalog (installed from catalog {})",
            source.id, entry.version
        ))
        .with_hint("delete it, or reinstall it from a path"));
    };
    let latest = manifest.version.clone();

    // A hand-edited `mcpServers.<id>` entry counts as local drift too:
    // reinstalling is also how the canonical entry is restored, so it must
    // not hide behind an early "up to date" — and must not be clobbered
    // without --force.
    let entry_drifted = target_ids.iter().any(|target_id| {
        entry
            .targets
            .get(target_id)
            .and_then(|target| target.managed_mcp_entry.as_ref())
            .is_some_and(|managed| {
                lockfile::managed_mcp_entry_status(scope_root, id, managed) != "clean"
            })
    });

    if latest == entry.version && !entry_drifted {
        println!("'{}' is already up to date (catalog {latest})", id);
        return Ok(());
    }

    let mut all_clean = true;
    for target_id in target_ids {
        let target_entry = entry.targets.get(target_id).ok_or_else(|| {
            TuffError::not_found(format!(
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
        if entry_drifted {
            println!(
                "'{}' has a hand-edited MCP config entry — update --force would restore the canonical entry",
                id
            );
        } else if all_clean {
            println!(
                "'{}' can be updated cleanly: catalog {} → {latest}",
                id, entry.version
            );
        } else {
            println!(
                "'{}' has local changes — update would replace the materialized tree",
                id
            );
        }
        return Ok(());
    }

    if (!all_clean || entry_drifted) && !force {
        return Err(
            TuffError::drift(format!("'{id}' has local changes")).with_hint(format!(
                "run 'tuff diff {id}' first, or use --force to reload from the catalog"
            )),
        );
    }

    let capability = resolve_capability(&manifest)?;
    install_capability(
        scope_root,
        scope,
        &capability,
        &manifest,
        target_ids,
        Some(CapabilitySource::Catalog(CatalogSource {
            id: source.id.clone(),
            version: latest,
            registry: source.registry.clone(),
        })),
        true,
    )
}

pub struct UpdateOptions<'a> {
    pub scope: Option<&'a str>,
    pub requested_targets: &'a [String],
    pub check: bool,
    pub force: bool,
    /// Pack artifact to update from instead of resolving the pack's
    /// registry; only meaningful for a pack-installed capability.
    pub pack_artifact: Option<&'a Path>,
    pub oci_options: OciTransferOptions,
}

pub fn cmd_update(repo_root: &Path, id: &str, options: UpdateOptions<'_>) -> Result<()> {
    let UpdateOptions {
        scope: scope_str,
        requested_targets,
        check,
        force,
        pack_artifact,
        oci_options,
    } = options;
    let (scope, entry, scope_root) = if let Some(s) = scope_str {
        let scope = resolver::Scope::parse(s)
            .ok_or_else(|| TuffError::usage(format!("invalid scope '{}'", s)))?;
        let root = match scope {
            Scope::Project => repo_root.to_path_buf(),
            Scope::Global => home_dir()?,
        };
        let lf = lockfile::require_scoped_lockfile(&root, scope)?;
        let entry = lf.capabilities.get(id).ok_or_else(|| {
            TuffError::not_found(format!(
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
                return Err(TuffError::not_found(format!("'{}' is not installed", id)));
            }
        }
    };

    if matches!(entry.source, CapabilitySource::Pack(_)) {
        // Pack members move with their pack; see `cmd_update_pack`.
        if scope == Scope::Global {
            return Err(TuffError::unsupported(format!(
                "'{id}' is a pack member; packs are installed in project scope only"
            )));
        }
        return super::pack::cmd_update_pack(super::pack::PackUpdateRequest {
            repo_root: &scope_root,
            id,
            requested_targets,
            check,
            force,
            artifact: pack_artifact,
            oci_options: &oci_options,
        });
    }
    if pack_artifact.is_some() {
        return Err(TuffError::usage(format!(
            "--pack only applies to a capability installed from a pack; '{id}' was not"
        )));
    }

    let target_ids =
        resolve_agent_selection(&scope_root, requested_targets, scope == Scope::Global)?;

    let git = match &entry.source {
        CapabilitySource::Local(local) if is_in_place_source_path(&local.path) => {
            return update_local_baseline(&scope_root, scope, id, &target_ids, check, force);
        }
        CapabilitySource::Local(_) => {
            return update_local_from_source(
                &scope_root,
                scope,
                id,
                &entry,
                &target_ids,
                check,
                force,
            );
        }
        CapabilitySource::Catalog(_) => {
            return update_from_catalog(&scope_root, scope, id, &entry, &target_ids, check, force);
        }
        CapabilitySource::Git(git) => git,
        CapabilitySource::Pack(_) => unreachable!("pack members are dispatched above"),
    };

    let (_source_guard, cache_dir, _clean_url) = git::clone_to_temp(&git.url, None)?;
    let latest_sha = git::resolve_ref(&cache_dir)?;

    if latest_sha == entry.version {
        println!("'{}' is already up to date", id);
        return Ok(());
    }

    let skill_dir = git::discover_capability(&cache_dir, &git.path, entry.capability_type)?;

    if force {
        let manifest = manifest::synthetic_manifest(&skill_dir, id, &latest_sha)?;
        let capability = resolve_capability(&manifest)?;
        return install_capability(
            &scope_root,
            scope,
            &capability,
            &manifest,
            &target_ids,
            Some(CapabilitySource::Git(GitSource {
                url: git.url.clone(),
                path: git.path.clone(),
                git_ref: latest_sha,
                tag: None,
                requested: None,
            })),
            true,
        );
    }

    let mut all_clean = true;
    for target_id in &target_ids {
        let target_entry = entry.targets.get(target_id).ok_or_else(|| {
            TuffError::not_found(format!(
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
        return Err(
            TuffError::drift(format!("'{id}' has local changes")).with_hint(format!(
                "run 'tuff diff {id}' first, or use --force to reload from source"
            )),
        );
    }

    let manifest = manifest::synthetic_manifest(&skill_dir, id, &latest_sha)?;
    let primitive = resolve_capability(&manifest)?;
    install_capability(
        &scope_root,
        scope,
        &primitive,
        &manifest,
        &target_ids,
        Some(CapabilitySource::Git(GitSource {
            url: git.url.clone(),
            path: git.path.clone(),
            git_ref: latest_sha,
            tag: None,
            requested: None,
        })),
        true,
    )
}
