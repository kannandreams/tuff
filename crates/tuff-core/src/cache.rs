use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{Result, TuffError};

pub fn cache_root(home: &Path) -> PathBuf {
    crate::paths::user_cache(home).join("sha256")
}

pub fn cache_path(home: &Path, hash: &str) -> Result<PathBuf> {
    validate_hash(hash)?;
    Ok(cache_root(home).join(&hash[..2]).join(hash))
}

pub fn validate_hash(hash: &str) -> Result<()> {
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(TuffError::new(format!("invalid capability hash: {hash}")));
    }
    Ok(())
}

/// Hashes a materialized directory using sorted relative paths and file bytes.
/// Directory metadata and filesystem iteration order are intentionally ignored.
pub fn hash_tree(root: &Path) -> Result<String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = Sha256::new();
    for (path, content) in files {
        let path = path.to_string_lossy().replace('\\', "/");
        hasher.update((path.len() as u64).to_be_bytes());
        hasher.update(path.as_bytes());
        hasher.update((content.len() as u64).to_be_bytes());
        hasher.update(content);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn populate(home: &Path, hash: &str, source: &Path) -> Result<PathBuf> {
    let destination = cache_path(home, hash)?;
    let actual = hash_tree(source)?;
    if actual != hash {
        return Err(TuffError::new(format!(
            "materialized capability hash mismatch: expected {hash}, got {actual}"
        )));
    }

    if destination.is_dir() {
        if hash_tree(&destination)? == hash {
            return Ok(destination);
        }
        if let Err(error) = std::fs::remove_dir_all(&destination) {
            if error.kind() == std::io::ErrorKind::PermissionDenied {
                return Ok(source.to_path_buf());
            }
            return Err(error.into());
        }
    }

    let parent = destination
        .parent()
        .ok_or_else(|| TuffError::new("invalid cache destination"))?;
    if let Err(error) = std::fs::create_dir_all(parent) {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            return Ok(source.to_path_buf());
        }
        return Err(error.into());
    }
    let temporary = match tempfile::Builder::new()
        .prefix("tuff-cache-")
        .tempdir_in(parent)
    {
        Ok(temporary) => temporary,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Ok(source.to_path_buf());
        }
        Err(error) => return Err(error.into()),
    };
    if let Err(error) = copy_tree(source, temporary.path()) {
        if error.to_string().contains("Permission denied") {
            return Ok(source.to_path_buf());
        }
        return Err(error);
    }
    let temporary_path = temporary.keep();
    if let Err(error) = std::fs::rename(&temporary_path, &destination) {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            return Ok(source.to_path_buf());
        }

        // Another process may have populated the same content-addressed
        // directory between our existence check and the rename. Reuse its
        // verified result instead of treating that harmless race as a cache
        // failure.
        if destination.is_dir() && hash_tree(&destination).ok().as_deref() == Some(hash) {
            let _ = std::fs::remove_dir_all(&temporary_path);
            return Ok(destination);
        }

        return Err(error.into());
    }
    Ok(destination)
}

pub fn read_verified(home: &Path, hash: &str) -> Result<Option<PathBuf>> {
    let path = cache_path(home, hash)?;
    if !path.is_dir() {
        return Ok(None);
    }
    if hash_tree(&path)? != hash {
        return Ok(None);
    }
    Ok(Some(path))
}

pub fn clear(home: &Path) -> Result<()> {
    let root = crate::paths::user_cache(home);
    if root.exists() {
        std::fs::remove_dir_all(root)?;
    }
    Ok(())
}

fn collect_files(root: &Path, current: &Path, output: &mut Vec<(PathBuf, Vec<u8>)>) -> Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, output)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| TuffError::new(error.to_string()))?
                .to_path_buf();
            output.push((relative, std::fs::read(path)?));
        }
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_tree(&source_path, &destination_path)?;
        } else if source_path.is_file() {
            std::fs::copy(source_path, destination_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn hash_tree_is_independent_of_creation_order() {
        let left = TempDir::new().unwrap();
        let right = TempDir::new().unwrap();
        std::fs::create_dir_all(left.path().join("nested")).unwrap();
        std::fs::create_dir_all(right.path().join("nested")).unwrap();
        std::fs::write(left.path().join("a"), "a").unwrap();
        std::fs::write(left.path().join("nested/b"), "b").unwrap();
        std::fs::write(right.path().join("nested/b"), "b").unwrap();
        std::fs::write(right.path().join("a"), "a").unwrap();
        assert_eq!(
            hash_tree(left.path()).unwrap(),
            hash_tree(right.path()).unwrap()
        );
    }

    #[test]
    fn cache_round_trip_verifies_content() {
        let home = TempDir::new().unwrap();
        let source = TempDir::new().unwrap();
        std::fs::write(source.path().join("file"), "content").unwrap();
        let hash = hash_tree(source.path()).unwrap();
        let path = populate(home.path(), &hash, source.path()).unwrap();
        assert_eq!(read_verified(home.path(), &hash).unwrap(), Some(path));
    }

    #[test]
    fn concurrent_populate_reuses_existing_content_addressed_directory() {
        let home = TempDir::new().unwrap();
        let source = TempDir::new().unwrap();
        std::fs::write(source.path().join("file"), "content").unwrap();
        let hash = hash_tree(source.path()).unwrap();
        let home_path = home.path().to_path_buf();
        let source_path = source.path().to_path_buf();

        let workers = (0..8)
            .map(|_| {
                let home_path = home_path.clone();
                let source_path = source_path.clone();
                let hash = hash.clone();
                std::thread::spawn(move || populate(&home_path, &hash, &source_path))
            })
            .collect::<Vec<_>>();

        for worker in workers {
            worker.join().unwrap().unwrap();
        }

        assert!(read_verified(home.path(), &hash).unwrap().is_some());
    }
}
