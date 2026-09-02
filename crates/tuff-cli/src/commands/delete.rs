use std::path::{Path, PathBuf};

use crate::adapter::{AdapterKind, AgentAdapter};
use crate::error::{Result, TuffError};
use crate::lockfile;
use crate::resolver::{self, Scope};

use super::{capability_index, home_dir, resolve_agent_selection};

fn resolve_cleanup_scope(repo_root: &Path, scope_str: &str) -> Result<(Scope, PathBuf)> {
    let scope = resolver::Scope::parse(scope_str)
        .ok_or_else(|| TuffError::new(format!("invalid scope '{}'", scope_str)))?;
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
        return Err(TuffError::new(format!(
            "'{}' is not tracked for agent '{}'",
            id, target
        )));
    }
    Ok(())
}

/// Where a tracked target has drifted from its recorded baseline.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LocalModifications {
    /// The installed tree or an emitted file differs from baseline.
    pub files: bool,
    /// A managed hook registration or MCP entry differs from baseline.
    pub managed: bool,
}

impl LocalModifications {
    pub(crate) fn any(self) -> bool {
        self.files || self.managed
    }
}

/// Check one tracked target for local edits, the same way `delete` and a
/// pack update decide whether `--force` is needed.
pub(crate) fn local_modifications(
    scope_root: &Path,
    id: &str,
    target_entry: &lockfile::TargetLockEntry,
) -> LocalModifications {
    let modified_tree = !target_entry.installed_path.is_empty()
        && crate::cache::hash_tree(&scope_root.join(&target_entry.installed_path))
            .map(|hash| hash != target_entry.sha256)
            .unwrap_or(true);
    let files = modified_tree;
    let managed = target_entry
        .managed_hooks
        .iter()
        .any(|hook| lockfile::managed_hook_status(scope_root, hook) == "modified")
        || target_entry
            .managed_mcp_entry
            .as_ref()
            .is_some_and(|entry| {
                lockfile::managed_mcp_entry_status(scope_root, id, entry) == "modified"
            });
    LocalModifications { files, managed }
}

pub fn cmd_delete(
    repo_root: &Path,
    id: &str,
    scope_str: &str,
    targets: &[String],
    force: bool,
) -> Result<()> {
    let (scope, scope_root) = resolve_cleanup_scope(repo_root, scope_str)?;
    let target_ids = resolve_agent_selection(&scope_root, targets, scope == Scope::Global)?;

    let mut lf = lockfile::require_scoped_lockfile(&scope_root, scope)?;
    let mut entry = lf.capabilities.get(id).cloned().ok_or_else(|| {
        TuffError::new(format!(
            "'{}' is not installed in {} scope",
            id,
            scope.as_str()
        ))
    })?;

    for target in &target_ids {
        let target_entry = entry.targets.get(target).ok_or_else(|| {
            TuffError::new(format!("'{}' is not tracked for agent '{}'", id, target))
        })?;

        if target_entry.ownership == lockfile::TargetOwnership::Imported {
            return Err(TuffError::new(format!(
                "'{}' is tracked in place for agent '{}'; use 'tuff untrack {} -a {}' instead",
                id, target, id, target
            )));
        }

        let LocalModifications {
            files: modified,
            managed: modified_hook,
        } = local_modifications(&scope_root, id, target_entry);
        if (modified || modified_hook) && !force {
            return Err(TuffError::new(format!(
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
    lockfile::write_scoped_lockfile(&scope_root, scope, &lf)?;
    if id != capability_index::CAPABILITY_INDEX_ID {
        capability_index::regenerate_capability_index(&scope_root, scope)?;
    }
    println!("deleted '{}' from {} scope", id, scope.as_str());
    Ok(())
}

pub fn cmd_untrack(repo_root: &Path, id: &str, scope_str: &str, targets: &[String]) -> Result<()> {
    let (scope, scope_root) = resolve_cleanup_scope(repo_root, scope_str)?;
    let target_ids = resolve_agent_selection(&scope_root, targets, scope == Scope::Global)?;

    let mut lf = lockfile::require_scoped_lockfile(&scope_root, scope)?;
    let mut entry = lf.capabilities.get(id).cloned().ok_or_else(|| {
        TuffError::new(format!(
            "'{}' is not installed in {} scope",
            id,
            scope.as_str()
        ))
    })?;

    for target in &target_ids {
        if !entry.targets.contains_key(target) {
            return Err(TuffError::new(format!(
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
    lockfile::write_scoped_lockfile(&scope_root, scope, &lf)?;
    if id != capability_index::CAPABILITY_INDEX_ID {
        capability_index::regenerate_capability_index(&scope_root, scope)?;
    }
    println!("untracked '{}' from {} scope", id, scope.as_str());
    Ok(())
}
