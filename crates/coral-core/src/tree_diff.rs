use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use git2::{DiffFormat, DiffOptions, Repository, Tree};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use crate::error::{CoralError, Result};

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileChange {
    pub path: String,
    pub status: String,
    pub old_hash: Option<String>,
    pub new_hash: Option<String>,
}

#[derive(Debug)]
pub struct TreeComparison {
    pub patch: String,
    pub changes: Vec<FileChange>,
}

/// Imports both directories into one scoped temporary Git repository and compares
/// the resulting Git trees through libgit2. The repository is deleted with `temp`.
pub fn compare(old_root: &Path, new_root: &Path) -> Result<TreeComparison> {
    let temp = TempDir::new()?;
    let repository = Repository::init(temp.path()).map_err(git_error)?;
    let old_tree = write_tree(&repository, old_root)?;
    let new_tree = write_tree(&repository, new_root)?;

    let mut options = DiffOptions::new();
    let diff = repository
        .diff_tree_to_tree(Some(&old_tree), Some(&new_tree), Some(&mut options))
        .map_err(git_error)?;
    let mut patch = Vec::new();
    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        match line.origin() {
            '+' | '-' | ' ' => {
                patch.push(line.origin() as u8);
                patch.extend_from_slice(line.content());
            }
            _ => patch.extend_from_slice(line.content()),
        }
        true
    })
    .map_err(git_error)?;

    let old_files = file_hashes(old_root)?;
    let new_files = file_hashes(new_root)?;
    let mut changes = Vec::new();
    let mut paths = old_files.keys().cloned().collect::<Vec<_>>();
    paths.extend(
        new_files
            .keys()
            .filter(|path| !old_files.contains_key(*path))
            .cloned(),
    );
    paths.sort();
    paths.dedup();
    for path in paths {
        let old_hash = old_files.get(&path).cloned();
        let new_hash = new_files.get(&path).cloned();
        if old_hash == new_hash {
            continue;
        }
        let status = match (&old_hash, &new_hash) {
            (None, Some(_)) => "added",
            (Some(_), None) => "removed",
            (Some(_), Some(_)) => "modified",
            (None, None) => unreachable!(),
        };
        changes.push(FileChange {
            path,
            status: status.to_string(),
            old_hash,
            new_hash,
        });
    }

    Ok(TreeComparison {
        patch: String::from_utf8_lossy(&patch).into_owned(),
        changes,
    })
}

fn write_tree<'repo>(repository: &'repo Repository, root: &Path) -> Result<Tree<'repo>> {
    let workdir = repository
        .workdir()
        .ok_or_else(|| CoralError::new("temporary git repository has no workdir"))?;
    for entry in std::fs::read_dir(workdir)? {
        let entry = entry?;
        if entry.file_name() == ".git" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            std::fs::remove_dir_all(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
    }
    copy_tree(root, workdir)?;

    let mut index = repository.index().map_err(git_error)?;
    index.clear().map_err(git_error)?;
    let mut files = Vec::new();
    collect_paths(root, root, &mut files)?;
    files.sort();
    for path in files {
        index.add_path(&path).map_err(git_error)?;
    }
    index.write().map_err(git_error)?;
    let tree_id = index.write_tree().map_err(git_error)?;
    repository.find_tree(tree_id).map_err(git_error)
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            std::fs::create_dir_all(&destination_path)?;
            copy_tree(&source_path, &destination_path)?;
        } else if source_path.is_file() {
            std::fs::copy(source_path, destination_path)?;
        }
    }
    Ok(())
}

fn collect_paths(root: &Path, current: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(current)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_paths(root, &path, output)?;
        } else if path.is_file() {
            output.push(
                path.strip_prefix(root)
                    .map(PathBuf::from)
                    .map_err(|error| CoralError::new(error.to_string()))?,
            );
        }
    }
    Ok(())
}

fn file_hashes(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut paths = Vec::new();
    collect_paths(root, root, &mut paths)?;
    let mut result = BTreeMap::new();
    for relative in paths {
        let content = std::fs::read(root.join(&relative))?;
        let mut hasher = Sha256::new();
        hasher.update(content);
        result.insert(
            relative.to_string_lossy().replace('\\', "/"),
            format!("{:x}", hasher.finalize()),
        );
    }
    Ok(result)
}

fn git_error(error: git2::Error) -> CoralError {
    CoralError::new(format!("git2 diff failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn compares_added_removed_and_modified_files() {
        let old = TempDir::new().unwrap();
        let new = TempDir::new().unwrap();
        std::fs::write(old.path().join("modified"), "old").unwrap();
        std::fs::write(old.path().join("removed"), "gone").unwrap();
        std::fs::write(new.path().join("modified"), "new").unwrap();
        std::fs::write(new.path().join("added"), "new file").unwrap();

        let result = compare(old.path(), new.path()).unwrap();
        assert_eq!(result.changes.len(), 3);
        assert!(result.changes.iter().any(|change| change.status == "added"));
        assert!(
            result
                .changes
                .iter()
                .any(|change| change.status == "removed")
        );
        assert!(
            result
                .changes
                .iter()
                .any(|change| change.status == "modified")
        );
    }
}
