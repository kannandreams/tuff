use std::path::{Path, PathBuf};

use crate::adapter::{AdapterKind, AgentAdapter};
use crate::error::{CoralError, Result};
use crate::lockfile;
use crate::resolver::{self, Scope};

use super::{home_dir, resolve_agent_selection};

fn resolve_cleanup_scope(repo_root: &Path, scope_str: &str) -> Result<(Scope, PathBuf)> {
    let scope = resolver::Scope::from_str(scope_str)
        .ok_or_else(|| CoralError::new(format!("invalid scope '{}'", scope_str)))?;
    let scope_root = match scope {
        Scope::Project => repo_root.to_path_buf(),
        Scope::Global => home_dir()?,
    };
    Ok((scope, scope_root))
}

fn remove_target_tracking(
    _scope_root: &Path,
    id: &str,
    entry: &mut lockfile::CapabilityLockEntry,
    target: &str,
) -> Result<()> {
    if entry.targets.remove(target).is_none() {
        return Err(CoralError::new(format!(
            "'{}' is not tracked for agent '{}'",
            id, target
        )));
    }
    Ok(())
}

pub fn cmd_delete(
    repo_root: &Path,
    id: &str,
    scope_str: &str,
    targets: &[String],
    force: bool,
) -> Result<()> {
    let (scope, scope_root) = resolve_cleanup_scope(repo_root, scope_str)?;
    let target_ids = resolve_agent_selection(&scope_root, targets)?;

    let mut lf = lockfile::require_lockfile(&scope_root)?;
    let mut entry = lf.capabilities.get(id).cloned().ok_or_else(|| {
        CoralError::new(format!(
            "'{}' is not installed in {} scope",
            id,
            scope.as_str()
        ))
    })?;

    for target in &target_ids {
        let target_entry = entry.targets.get(target).ok_or_else(|| {
            CoralError::new(format!("'{}' is not tracked for agent '{}'", id, target))
        })?;

        if target_entry.ownership == lockfile::TargetOwnership::Imported {
            return Err(CoralError::new(format!(
                "'{}' is tracked in place for agent '{}'; use 'coral untrack {} -a {}' instead",
                id, target, id, target
            )));
        }

        let modified = target_entry
            .emitted_files
            .iter()
            .any(|emitted| lockfile::drift_status(&scope_root, emitted) == "modified");
        let modified_hook = target_entry
            .managed_hooks
            .iter()
            .any(|hook| lockfile::managed_hook_status(&scope_root, hook) == "modified");
        if (modified || modified_hook) && !force {
            return Err(CoralError::new(format!(
                "'{}' has local modifications for agent '{}'; use --force to delete",
                id, target
            )));
        }
        if modified {
            eprintln!(
                "warning: '{}' has local modifications for agent '{}' - deleting them",
                id, target
            );
        }
    }

    for target in &target_ids {
        if let Some(adapter) = AdapterKind::from_id(target) {
            let managed_hooks = entry
                .targets
                .get(target)
                .map(|target_entry| target_entry.managed_hooks.as_slice())
                .unwrap_or(&[]);
            adapter.remove(id, &scope_root, managed_hooks)?;
        }
        remove_target_tracking(&scope_root, id, &mut entry, target)?;
    }

    if entry.targets.is_empty() {
        lf.capabilities.remove(id);
    } else {
        lf.capabilities.insert(id.to_string(), entry);
    }
    lockfile::write_lockfile(&scope_root, &lf)?;
    lockfile::prune_unreferenced_baseline_objects(&scope_root, &lf)?;
    println!("deleted '{}' from {} scope", id, scope.as_str());
    Ok(())
}

pub fn cmd_untrack(repo_root: &Path, id: &str, scope_str: &str, targets: &[String]) -> Result<()> {
    let (scope, scope_root) = resolve_cleanup_scope(repo_root, scope_str)?;
    let target_ids = resolve_agent_selection(&scope_root, targets)?;

    let mut lf = lockfile::require_lockfile(&scope_root)?;
    let mut entry = lf.capabilities.get(id).cloned().ok_or_else(|| {
        CoralError::new(format!(
            "'{}' is not installed in {} scope",
            id,
            scope.as_str()
        ))
    })?;

    for target in &target_ids {
        if !entry.targets.contains_key(target) {
            return Err(CoralError::new(format!(
                "'{}' is not tracked for agent '{}'",
                id, target
            )));
        }
    }

    for target in &target_ids {
        remove_target_tracking(&scope_root, id, &mut entry, target)?;
    }

    if entry.targets.is_empty() {
        lf.capabilities.remove(id);
    } else {
        lf.capabilities.insert(id.to_string(), entry);
    }
    lockfile::write_lockfile(&scope_root, &lf)?;
    lockfile::prune_unreferenced_baseline_objects(&scope_root, &lf)?;
    println!("untracked '{}' from {} scope", id, scope.as_str());
    Ok(())
}
