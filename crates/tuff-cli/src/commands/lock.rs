use std::path::Path;

use crate::error::Result;
use crate::lockfile;

/// Rewrite the project lockfile in the current schema, changing nothing
/// else, so a team can land the migration as its own reviewable commit
/// (RFC-105 D5). A lockfile already at the current version is left alone.
pub fn cmd_lock_migrate(repo_root: &Path) -> Result<()> {
    let path = lockfile::project_lockfile(repo_root);
    let lock = lockfile::read_lockfile_at(&path)?;
    if lock.version == lockfile::LOCKFILE_VERSION {
        println!(
            "{} is already schema version {}",
            path.display(),
            lockfile::LOCKFILE_VERSION
        );
        return Ok(());
    }
    lockfile::write_lockfile_at(&path, &lock)?;
    println!(
        "migrated {} from schema version {} to {}",
        path.display(),
        lock.version,
        lockfile::LOCKFILE_VERSION
    );
    Ok(())
}
