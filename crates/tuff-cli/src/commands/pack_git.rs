//! Packs installed from a git repository (RFC-110 D3 and D4).
//!
//! A git pack is a repository folder holding a standalone `tuff-pack.toml`.
//! Tuff checks the folder out at one commit, builds an artifact from it, and
//! installs that artifact the way `tuff add pack <file>` does. The lockfile
//! pins the commit. Release tags use RFC-101's shapes, matched against the
//! last component of the pack name.

use std::path::{Path, PathBuf};

use semver::Version;
use tempfile::TempDir;

use crate::error::{Result, TuffError};
use crate::git;
use crate::lockfile::PackGitSource;
use crate::pack;
use crate::release;

/// A pack folder checked out at one commit. The checkout is deleted when
/// this is dropped.
pub(crate) struct GitPackCheckout {
    _guard: TempDir,
    /// The folder holding `tuff-pack.toml`.
    pub(crate) pack_root: PathBuf,
    pub(crate) source: PackGitSource,
}

/// The name release tags are scoped by: the last component of a pack name
/// such as `acme/review`.
pub(crate) fn release_name(pack_name: &str) -> &str {
    pack_name.rsplit('/').next().unwrap_or(pack_name)
}

/// Check out the pack at `url`, in `path` (or the folder the URL names),
/// at `requested` (a tag, branch, or commit) or else at its newest release.
pub(crate) fn checkout(
    url: &str,
    path: Option<&str>,
    requested: Option<&str>,
) -> Result<GitPackCheckout> {
    let subdir = match path {
        Some(path) => Some(path.trim_matches('/').to_string()),
        None => git::source_subdirectory(url),
    }
    .filter(|path| !path.is_empty());
    if let Some(subdir) = subdir.as_deref() {
        crate::tool::check_path_traversal(subdir)?;
    }
    let tags = git::list_remote_tags(url)?;

    let (guard, checkout, clean_url, tag, branch) = match requested {
        Some(reference) if tags.iter().any(|tag| tag.name == reference) => {
            let (guard, checkout, clean_url) = git::clone_tag_to_temp(url, reference)?;
            (
                guard,
                checkout,
                clean_url,
                Some(reference.to_string()),
                None,
            )
        }
        Some(reference) if looks_like_commit(reference) => {
            let (guard, checkout, clean_url) = git::clone_to_temp(url, Some(reference))?;
            (guard, checkout, clean_url, None, None)
        }
        Some(branch) => {
            // `clone --branch` takes a branch as readily as a tag.
            let (guard, checkout, clean_url) = git::clone_tag_to_temp(url, branch)?;
            (guard, checkout, clean_url, None, Some(branch.to_string()))
        }
        None => {
            let (guard, checkout, clean_url) = git::clone_to_temp(url, None)?;
            // The release tags are scoped by the pack's name, which only the
            // checkout can tell.
            let name = manifest_at(&checkout, subdir.as_deref(), &clean_url)?.name;
            let releases = release::release_tags(
                tags.iter().map(|tag| tag.name.as_str()),
                release_name(&name),
            );
            match release::latest_release(&releases) {
                Some(latest) => {
                    let latest = latest.tag.clone();
                    drop(guard);
                    let (guard, checkout, clean_url) = git::clone_tag_to_temp(url, &latest)?;
                    (guard, checkout, clean_url, Some(latest), None)
                }
                None => {
                    let branch = git::current_branch(&checkout);
                    (guard, checkout, clean_url, None, branch)
                }
            }
        }
    };

    let commit = git::resolve_ref(&checkout)?;
    let manifest = manifest_at(&checkout, subdir.as_deref(), &clean_url)?;
    if let Some(tag) = tag.as_deref() {
        check_tag_version(tag, &manifest)?;
    }
    let pack_root = match subdir.as_deref() {
        Some(subdir) => checkout.join(subdir),
        None => checkout,
    };
    Ok(GitPackCheckout {
        _guard: guard,
        pack_root,
        source: PackGitSource {
            url: clean_url,
            path: subdir.unwrap_or_default(),
            git_ref: commit,
            tag,
            branch,
        },
    })
}

/// The repository chose the folder and the manifest, so neither may be a
/// link that leads out of the checkout to a file on this machine.
fn contained_in_checkout(checkout: &Path, pack_root: &Path) -> Result<()> {
    let refuse = || {
        TuffError::refused(format!(
            "the pack folder or its {} is a symbolic link out of the repository",
            pack::PACK_MANIFEST_FILE
        ))
    };
    let checkout = checkout.canonicalize()?;
    if !pack_root.canonicalize()?.starts_with(&checkout) {
        return Err(refuse());
    }
    let manifest = pack_root.join(pack::PACK_MANIFEST_FILE);
    if std::fs::symlink_metadata(&manifest)?
        .file_type()
        .is_symlink()
    {
        return Err(refuse());
    }
    Ok(())
}

fn looks_like_commit(reference: &str) -> bool {
    (7..=40).contains(&reference.len()) && reference.chars().all(|c| c.is_ascii_hexdigit())
}

/// The pack manifest in the checkout, or an error naming where it was looked
/// for. Only the manifest is read here; members are loaded by the caller.
fn manifest_at(checkout: &Path, subdir: Option<&str>, url: &str) -> Result<pack::PackManifest> {
    let folder = match subdir {
        Some(subdir) => checkout.join(subdir),
        None => checkout.to_path_buf(),
    };
    if !folder.join(pack::PACK_MANIFEST_FILE).is_file() {
        let place = subdir.map_or_else(
            || "the repository root".to_string(),
            |subdir| format!("'{subdir}'"),
        );
        return Err(TuffError::not_found(format!(
            "no {} at {place} in {url}",
            pack::PACK_MANIFEST_FILE
        ))
        .with_hint("name the folder holding the pack with --path, or in the URL"));
    }
    contained_in_checkout(checkout, &folder)?;
    let (_, manifest) = pack::load_manifest(&folder)?;
    if manifest.project.is_some() {
        return Err(TuffError::usage(format!(
            "the pack at {url} selects capabilities from its author's project, which only its author can build"
        ))
        .with_hint("install a pack whose tuff-pack.toml lists [[capabilities]] paths"));
    }
    Ok(manifest)
}

/// A release tag names a version, so the manifest at that tag must declare
/// the same one. A tag that does not read as a release is taken as named.
fn check_tag_version(tag: &str, manifest: &pack::PackManifest) -> Result<()> {
    let Some(release) = release::parse_release_tag(tag, release_name(&manifest.name)) else {
        return Ok(());
    };
    let declared = Version::parse(&manifest.version).ok();
    if declared.as_ref() != Some(&release.version) {
        return Err(TuffError::refused(format!(
            "tag {tag} names version {}, but the tuff-pack.toml at that tag declares version {}",
            release.version, manifest.version
        ))
        .with_hint(
            "the pack's author should tag the commit whose manifest declares that version",
        ));
    }
    Ok(())
}

/// What the remote says about an installed git pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GitPackStatus {
    /// A newer release tag exists.
    Newer { tag: String, version: String },
    /// The install is the newest release, or its branch has not moved.
    Current,
    /// The installed tag now names a different commit.
    Repointed { live_commit: String },
    /// The installed tag no longer exists.
    TagMissing,
    /// The followed branch names a newer commit.
    BranchMoved { commit: String },
    /// Installed from a bare commit: there is nothing to follow.
    Pinned,
}

/// Compare an installed git pack with its remote. Integrity findings win
/// over a newer release, as they do for registry packs: a repointed or
/// deleted tag changes what the installed version means. The newest release
/// is still returned with them so the way forward is visible.
pub(crate) fn status(
    pack_name: &str,
    installed_version: &str,
    source: &PackGitSource,
) -> Result<(GitPackStatus, Option<release::ReleaseTag>)> {
    if let Some(branch) = source.branch.as_deref() {
        let head = git::remote_branch_head(&source.url, branch)?.ok_or_else(|| {
            TuffError::not_found(format!(
                "branch {branch} no longer exists in {}",
                source.url
            ))
        })?;
        let status = if head == source.git_ref {
            GitPackStatus::Current
        } else {
            GitPackStatus::BranchMoved { commit: head }
        };
        return Ok((status, None));
    }
    let Some(tag) = source.tag.as_deref() else {
        return Ok((GitPackStatus::Pinned, None));
    };
    let tags = git::list_remote_tags(&source.url)?;
    let releases = release::release_tags(
        tags.iter().map(|tag| tag.name.as_str()),
        release_name(pack_name),
    );
    let latest = release::latest_release(&releases).cloned();
    let integrity = release::tag_integrity(tag, &source.git_ref, &tags);
    let status = match integrity {
        release::TagIntegrity::Repointed { live_commit } => {
            GitPackStatus::Repointed { live_commit }
        }
        release::TagIntegrity::Missing => GitPackStatus::TagMissing,
        release::TagIntegrity::Matches => {
            let installed = Version::parse(installed_version).ok();
            match &latest {
                Some(latest) if installed.as_ref().is_none_or(|v| &latest.version > v) => {
                    GitPackStatus::Newer {
                        tag: latest.tag.clone(),
                        version: latest.version.to_string(),
                    }
                }
                _ => GitPackStatus::Current,
            }
        }
    };
    Ok((status, latest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_tags_are_scoped_by_the_last_part_of_the_pack_name() {
        assert_eq!(release_name("acme/review"), "review");
        assert_eq!(release_name("review"), "review");
    }

    #[test]
    fn only_hexadecimal_references_of_commit_length_read_as_commits() {
        assert!(looks_like_commit("4b1e9c2"));
        assert!(looks_like_commit(&"a".repeat(40)));
        assert!(!looks_like_commit("main"));
        assert!(!looks_like_commit("abc"));
        assert!(!looks_like_commit("v1.2.0"));
    }
}
