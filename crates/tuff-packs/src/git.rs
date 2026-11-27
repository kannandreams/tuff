//! Packs installed from a git repository (RFC-110 D3, D4, and D12).
//!
//! A git pack is a repository folder holding a standalone `tuff-pack.toml`,
//! or a Claude Code plugin or marketplace, which is imported as a pack each
//! time it is installed or updated. Tuff checks the folder out at one
//! commit, builds an artifact from it, and installs that artifact the way
//! `tuff add pack <file>` does. The lockfile pins the commit. Release tags
//! use RFC-101's shapes, matched against the last component of the pack or
//! plugin name.

use std::path::{Path, PathBuf};

use semver::Version;
use tempfile::TempDir;

use tuff_adapter_claude::plugin::{self as claude_plugin, ClaudePlugin};
use tuff_core::bundle::{BundleLocation, BundleReader, LocalBundle, RemoteBundle};

use tuff_core::error::{Result, TuffError};
use tuff_core::git;
use tuff_core::lockfile::PackGitSource;
use tuff_core::pack;
use tuff_core::release;

/// What a checked-out folder holds.
pub enum CheckedOut {
    /// A standalone Tuff source pack at `pack_root`.
    Tuff,
    /// A Claude Code plugin, read from this bundle.
    ClaudePlugin(LocalBundle),
}

/// A pack folder checked out at one commit. The checkout is deleted when
/// this is dropped.
pub struct GitPackCheckout {
    _guards: Vec<TempDir>,
    /// The folder that was checked out: the one holding `tuff-pack.toml`,
    /// or the plugin or marketplace.
    pub pack_root: PathBuf,
    pub found: CheckedOut,
    pub source: PackGitSource,
    /// The version the release tag names, for a plugin that declares none.
    pub version_hint: Option<String>,
}

/// What to check out.
pub struct CheckoutRequest<'a> {
    pub url: &'a str,
    pub path: Option<&'a str>,
    /// A tag, branch, or commit; the newest release tag when absent.
    pub git_ref: Option<&'a str>,
    /// The marketplace entry to read.
    pub plugin: Option<&'a str>,
    /// Whether a marketplace entry may live in another repository. An
    /// install follows one repository's tags, so only an import allows it.
    pub allow_remote: bool,
}

/// The name release tags are scoped by: the last component of a pack name
/// such as `acme/review`.
pub fn release_name(pack_name: &str) -> &str {
    pack_name.rsplit('/').next().unwrap_or(pack_name)
}

/// What a folder holds, with the name and version that release tags are
/// read against.
enum Found {
    Tuff {
        name: String,
        version: String,
    },
    Plugin {
        bundle: LocalBundle,
        name: String,
        version: Option<String>,
    },
    Remote(RemoteBundle),
}

impl Found {
    fn name(&self) -> &str {
        match self {
            Self::Tuff { name, .. } => name,
            Self::Plugin { name, .. } => name,
            Self::Remote(remote) => &remote.name,
        }
    }
}

/// Check out `request.url`, in `request.path` (or the folder the URL
/// names), at `request.git_ref` or else at the newest release tag.
pub fn checkout(request: CheckoutRequest<'_>) -> Result<GitPackCheckout> {
    let CheckoutRequest {
        url,
        path,
        git_ref: requested,
        plugin,
        allow_remote,
    } = request;
    let subdir = match path {
        Some(path) => Some(path.trim_matches('/').to_string()),
        None => git::source_subdirectory(url),
    }
    .filter(|path| !path.is_empty());
    if let Some(subdir) = subdir.as_deref() {
        tuff_core::tool::check_path_traversal(subdir)?;
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
            let found = identify(&checkout, subdir.as_deref(), &clean_url, plugin)?;
            let releases = release::release_tags(
                tags.iter().map(|tag| tag.name.as_str()),
                release_name(found.name()),
            );
            match release::latest_release(&releases) {
                Some(latest) => {
                    let latest = latest.tag.clone();
                    drop(found);
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
    let found = identify(&checkout, subdir.as_deref(), &clean_url, plugin)?;
    let pack_root = match subdir.as_deref() {
        Some(subdir) => checkout.join(subdir),
        None => checkout,
    };
    let release_version = tag
        .as_deref()
        .and_then(|tag| release::parse_release_tag(tag, release_name(found.name())))
        .map(|release| (release.tag, release.version));
    let mut source = PackGitSource {
        url: clean_url,
        path: subdir.unwrap_or_default(),
        git_ref: commit,
        tag,
        branch,
        format: None,
        plugin: plugin.map(str::to_string),
    };
    let mut guards = vec![guard];
    let mut version_hint = None;
    let found = match found {
        Found::Tuff {
            version: declared, ..
        } => {
            if let Some((tag, version)) = &release_version {
                check_tag_version(tag, version, Some(&declared))?;
            }
            CheckedOut::Tuff
        }
        Found::Plugin {
            bundle, version, ..
        } => {
            source.format = Some(claude_plugin::FORMAT_ID.to_string());
            if let Some((tag, release)) = &release_version {
                check_tag_version(tag, release, version.as_deref())?;
                version_hint = Some(release.to_string());
            }
            CheckedOut::ClaudePlugin(bundle)
        }
        Found::Remote(remote) => {
            if !allow_remote {
                return Err(TuffError::unsupported(format!(
                    "marketplace entry '{}' lives in another repository, {}",
                    remote.name, remote.url
                ))
                .with_hint(
                    "install from that repository, or import the entry with 'tuff pack import --plugin'",
                ));
            }
            let (guard, bundle, remote_source) = checkout_remote(&remote)?;
            guards.push(guard);
            source = remote_source;
            CheckedOut::ClaudePlugin(bundle)
        }
    };
    Ok(GitPackCheckout {
        _guards: guards,
        pack_root,
        found,
        source,
        version_hint,
    })
}

/// Clone the repository a marketplace entry names, at the commit or ref it
/// names, and return the plugin folder in it.
pub fn checkout_remote(remote: &RemoteBundle) -> Result<(TempDir, LocalBundle, PackGitSource)> {
    let (guard, checkout, clean_url) = match (&remote.commit, &remote.git_ref) {
        (Some(commit), _) => git::clone_to_temp(&remote.url, Some(commit))?,
        (None, Some(reference)) => git::clone_tag_to_temp(&remote.url, reference)?,
        (None, None) => git::clone_to_temp(&remote.url, None)?,
    };
    let commit = git::resolve_ref(&checkout)?;
    let relative = match remote.path.as_deref() {
        Some(path) => tuff_core::bundle::bundle_path(path)?,
        None => PathBuf::new(),
    };
    let root = checkout.join(&relative);
    contained_in_checkout(&checkout, &root, None)?;
    // A listing's `ref` can name a tag or a branch; record which, so an
    // update knows whether to follow tags or the branch.
    let (tag, branch) = match remote.git_ref.as_deref() {
        Some(reference)
            if git::list_remote_tags(&clean_url)?
                .iter()
                .any(|tag| tag.name == reference) =>
        {
            (Some(reference.to_string()), None)
        }
        Some(reference) => (None, Some(reference.to_string())),
        None => (None, None),
    };
    let source = PackGitSource {
        url: clean_url,
        path: relative.to_string_lossy().replace('\\', "/"),
        git_ref: commit,
        tag: tag.clone(),
        branch,
        format: Some(claude_plugin::FORMAT_ID.to_string()),
        plugin: Some(remote.name.clone()),
    };
    let bundle = LocalBundle {
        root,
        name: Some(remote.name.clone()),
        entry: Some(remote.entry.clone()),
    };
    Ok((guard, bundle, source))
}

/// The repository chose the folder and its manifests, so none of them may
/// be a link that leads out of the checkout to a file on this machine.
fn contained_in_checkout(checkout: &Path, folder: &Path, manifest: Option<&str>) -> Result<()> {
    let refuse = || {
        TuffError::refused(
            "the pack folder or its manifest is a symbolic link out of the repository",
        )
    };
    let checkout = checkout.canonicalize()?;
    if !folder.canonicalize()?.starts_with(&checkout) {
        return Err(refuse());
    }
    if let Some(manifest) = manifest
        && std::fs::symlink_metadata(folder.join(manifest))?
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

/// What the checkout holds at the folder: a Tuff pack, a Claude Code plugin,
/// or a marketplace entry in this repository or another.
fn identify(
    checkout: &Path,
    subdir: Option<&str>,
    url: &str,
    plugin: Option<&str>,
) -> Result<Found> {
    let folder = match subdir {
        Some(subdir) => checkout.join(subdir),
        None => checkout.to_path_buf(),
    };
    let place = || {
        subdir.map_or_else(
            || "the repository root".to_string(),
            |subdir| format!("'{subdir}'"),
        )
    };
    if !folder.is_dir() {
        return Err(TuffError::not_found(format!(
            "{} is not a folder in {url}",
            place()
        )));
    }
    if folder.join(pack::PACK_MANIFEST_FILE).is_file() {
        contained_in_checkout(checkout, &folder, Some(pack::PACK_MANIFEST_FILE))?;
        if plugin.is_some() {
            return Err(TuffError::usage(
                "--plugin selects a marketplace entry, and this folder holds a Tuff pack",
            ));
        }
        let (_, manifest) = pack::load_manifest(&folder)?;
        if manifest.project.is_some() {
            return Err(TuffError::usage(format!(
                "the pack at {url} selects capabilities from its author's project, which only its author can build"
            ))
            .with_hint("install a pack whose tuff-pack.toml lists [[capabilities]] paths"));
        }
        return Ok(Found::Tuff {
            name: manifest.name,
            version: manifest.version,
        });
    }
    if ClaudePlugin.detect(&folder) {
        contained_in_checkout(checkout, &folder, None)?;
        return match ClaudePlugin.locate(&folder, plugin)? {
            BundleLocation::Local(bundle) => {
                contained_in_checkout(checkout, &bundle.root, None)?;
                let (name, version) = claude_plugin::identity(&bundle)?;
                Ok(Found::Plugin {
                    bundle,
                    name,
                    version,
                })
            }
            BundleLocation::Remote(remote) => Ok(Found::Remote(remote)),
        };
    }
    Err(TuffError::not_found(format!(
        "no {} or Claude Code plugin at {} in {url}",
        pack::PACK_MANIFEST_FILE,
        place()
    ))
    .with_hint("name the folder holding the pack with --path, or in the URL"))
}

/// A release tag names a version, so a source at that tag that declares a
/// version must declare the same one.
fn check_tag_version(tag: &str, release: &Version, declared: Option<&str>) -> Result<()> {
    let Some(declared) = declared else {
        return Ok(());
    };
    if Version::parse(declared).ok().as_ref() != Some(release) {
        return Err(TuffError::refused(format!(
            "tag {tag} names version {release}, but the manifest at that tag declares version {declared}"
        ))
        .with_hint("the author should tag the commit whose manifest declares that version"));
    }
    Ok(())
}

/// What the remote says about an installed git pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitPackStatus {
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
pub fn status(
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
