use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde::Serialize;
use tempfile::TempDir;

use crate::adapter::{AdapterKind, AgentAdapter, resolve_capability};
use crate::error::{Result, TuffError};
use crate::git;
use crate::lockfile::{self, CapabilitySource};
use crate::manifest::{self, load_manifest};
use crate::release::{self, ReleaseTag, VersionRequest};
use crate::resolver::{self, Scope};
use crate::tree_diff::{self, FileChange};

use super::{home_dir, resolve_agent_selection};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DiffFormat {
    Unified,
    Json,
}

#[derive(Debug)]
pub(crate) struct MaterializedTree {
    _sources: Vec<TempDir>,
    path: PathBuf,
}

#[derive(Debug, Serialize)]
struct JsonDiff {
    capability: String,
    target: String,
    /// The release tag an `--upstream` diff was taken against, when the
    /// entry is pinned to one (RFC-101); absent for a HEAD comparison.
    #[serde(skip_serializing_if = "Option::is_none")]
    upstream: Option<String>,
    changes: Vec<FileChange>,
}

/// Which commit of a git source to materialize.
enum GitCheckout<'a> {
    /// The commit the lockfile pinned.
    Pinned,
    /// The latest commit on the default branch.
    Head,
    /// One release tag.
    Tag(&'a str),
}

pub fn cmd_diff(
    repo_root: &Path,
    capability_id: &str,
    target: Option<&str>,
    upstream: bool,
    format: DiffFormat,
) -> Result<()> {
    // `<id>@<requirement>` previews a different requirement than the
    // recorded one, the way `update <id>@<requirement>` would move.
    let (capability_id, new_request) = release::split_version_request(capability_id)?;
    if new_request.is_some() && !upstream {
        return Err(
            TuffError::usage("a version requirement only applies with --upstream").with_hint(
                format!(
                    "run 'tuff diff {capability_id}@{} --upstream'",
                    new_request.unwrap_or_default()
                ),
            ),
        );
    }
    let (scope, entry, scope_root) = match resolver::resolve_entry(capability_id, repo_root)? {
        Some((scope, entry)) => {
            let root = match scope {
                Scope::Project => repo_root.to_path_buf(),
                Scope::Global => home_dir()?,
            };
            (scope, entry, root)
        }
        None => {
            return Err(TuffError::not_found(format!(
                "capability is not installed: {capability_id}"
            )));
        }
    };
    let _ = scope;

    let requested = target
        .map(|value| vec![value.to_string()])
        .unwrap_or_default();
    let targets = resolve_agent_selection(&scope_root, &requested, scope == Scope::Global)?;
    // A release-pinned entry is compared against the newest release its
    // requirement allows, never HEAD: the same answer `update` would give.
    let upstream_release = if upstream {
        upstream_release(capability_id, &entry, new_request)?
    } else {
        None
    };
    if let Some((release, request)) = &upstream_release {
        eprintln!(
            "comparing against {}, the newest release matching {request}",
            release.tag
        );
    }
    let mut unified = String::new();
    let mut json = Vec::new();

    for target in targets {
        let target_entry = entry.targets.get(&target).ok_or_else(|| {
            TuffError::not_found(format!(
                "'{capability_id}' is not installed for target '{target}'"
            ))
        })?;
        let live = installed_path(&scope_root, capability_id, &entry, target_entry, &target);
        let baseline = if upstream {
            let checkout = match &upstream_release {
                Some((release, _)) => GitCheckout::Tag(&release.tag),
                None => GitCheckout::Head,
            };
            materialize_upstream(&scope_root, capability_id, &entry, &target, checkout)?
        } else {
            materialize_baseline(&scope_root, capability_id, &entry, &target)?
        };
        let live_temp = TempDir::new()?;
        if live.is_dir() {
            copy_tree(&live, live_temp.path())?;
        }
        let comparison = tree_diff::compare(&baseline.path, live_temp.path())?;

        if matches!(format, DiffFormat::Json) {
            json.push(JsonDiff {
                capability: capability_id.to_string(),
                target,
                upstream: upstream_release
                    .as_ref()
                    .map(|(release, _)| release.tag.clone()),
                changes: comparison.changes,
            });
        } else if !comparison.patch.is_empty() {
            unified.push_str(&comparison.patch);
        }
    }

    match format {
        DiffFormat::Json => println!("{}", serde_json::to_string_pretty(&json)?),
        DiffFormat::Unified => {
            if unified.is_empty() && upstream {
                match &upstream_release {
                    Some((release, _)) => println!("no upstream changes in {}", release.tag),
                    None => println!("no upstream changes"),
                }
            } else if !unified.is_empty() {
                print!("{}", style_diff(&unified));
            }
        }
    }
    Ok(())
}

/// The release an `--upstream` diff compares against, with the requirement
/// that chose it: the one given on the command line, else the recorded
/// one. `None` for anything not pinned to a release, which compares
/// against HEAD as always.
fn upstream_release(
    id: &str,
    entry: &lockfile::CapabilityLockEntry,
    new_request: Option<&str>,
) -> Result<Option<(ReleaseTag, VersionRequest)>> {
    let CapabilitySource::Git(git) = &entry.source else {
        if new_request.is_some() {
            return Err(TuffError::usage(format!(
                "'{id}' was not installed from git, so a version requirement does not apply"
            )));
        }
        return Ok(None);
    };
    let request = match new_request.or(git.requested.as_deref()) {
        Some(text) => VersionRequest::parse(text)?,
        None => return Ok(None),
    };
    let release = super::resolve_git_release(&git.url, id, &request)?;
    Ok(Some((release, request)))
}

pub(crate) fn materialize_baseline(
    scope_root: &Path,
    id: &str,
    entry: &lockfile::CapabilityLockEntry,
    target: &str,
) -> Result<MaterializedTree> {
    let target_entry = entry.targets.get(target).ok_or_else(|| {
        TuffError::not_found(format!("no lock entry for '{id}' at target '{target}'"))
    })?;
    let expected = &target_entry.sha256;
    if expected.is_empty() {
        return Err(TuffError::corrupt(format!(
            "lock entry for '{id}' has no materialized baseline hash"
        )));
    }
    let home = super::home_dir()?;
    if let Some(path) = crate::cache::read_verified(&home, expected)? {
        return Ok(MaterializedTree {
            _sources: Vec::new(),
            path,
        });
    }

    let materialized = materialize_source(scope_root, id, entry, target, GitCheckout::Pinned)?;
    let actual = crate::cache::hash_tree(&materialized.path)?;
    if actual != *expected {
        if let Some(source_path) = entry.source.local_path()
            && !Path::new(source_path).is_absolute()
            && !scope_root.join(source_path).exists()
        {
            return Err(TuffError::not_found(format!(
                "The recorded baseline for \"{id}\" is not cached, and its local source \"{source_path}\" is no longer available.\n\nRestore the source, reinstall the capability, or run the appropriate Tuff command to accept the current content as a new baseline."
            )));
        }
        return Err(TuffError::corrupt(format!(
            "recorded baseline verification failed for '{id}': expected {expected}, got {actual}; the source does not reproduce the recorded baseline"
        )));
    }
    let cached = crate::cache::populate(&home, expected, &materialized.path)?;
    Ok(MaterializedTree {
        _sources: materialized._sources,
        path: cached,
    })
}

fn materialize_upstream(
    scope_root: &Path,
    id: &str,
    entry: &lockfile::CapabilityLockEntry,
    target: &str,
    checkout: GitCheckout<'_>,
) -> Result<MaterializedTree> {
    match &entry.source {
        CapabilitySource::Local(_) | CapabilitySource::Pack(_) => {
            return Err(TuffError::unsupported(
                "upstream diff only available for git-sourced capabilities",
            ));
        }
        CapabilitySource::Catalog(_) => {
            return Err(TuffError::unsupported(
                "upstream diff is not available for catalog capabilities",
            )
            .with_hint("run 'tuff outdated' to compare against the built-in catalog"));
        }
        CapabilitySource::Git(_) => {}
    }
    materialize_source(scope_root, id, entry, target, checkout)
}

fn materialize_source(
    scope_root: &Path,
    id: &str,
    entry: &lockfile::CapabilityLockEntry,
    target: &str,
    checkout: GitCheckout<'_>,
) -> Result<MaterializedTree> {
    let adapter = AdapterKind::from_id(target)
        .ok_or_else(|| TuffError::usage(format!("unknown target '{target}'")))?;
    let missing_local = |source_path: &str| {
        TuffError::not_found(format!(
            "The recorded baseline for \"{id}\" is not cached, and its local source \"{source_path}\" is no longer available.\n\nRestore the source, reinstall the capability, or run the appropriate Tuff command to accept the current content as a new baseline."
        ))
    };

    // An adopted capability, or a pack member, has no source tree apart
    // from the installed one.
    let in_place = match &entry.source {
        CapabilitySource::Local(local) => is_in_place_source_path(&local.path),
        CapabilitySource::Pack(_) => true,
        CapabilitySource::Git(_) | CapabilitySource::Catalog(_) => false,
    };
    if in_place {
        let fallback = entry.source.local_path().unwrap_or_default();
        let path = entry
            .targets
            .get(target)
            .map(|target_entry| target_entry.installed_path.as_str())
            .filter(|path| !path.is_empty())
            .map(|path| scope_root.join(path))
            .unwrap_or_else(|| scope_root.join(fallback));
        if !path.is_dir() {
            return Err(missing_local(fallback));
        }
        return Ok(MaterializedTree {
            _sources: Vec::new(),
            path,
        });
    }

    let (source_guard, capability_dir) = match &entry.source {
        CapabilitySource::Catalog(catalog) => {
            let manifest = crate::catalog::lookup(&catalog.id)?.ok_or_else(|| {
                TuffError::not_found(format!(
                    "The recorded baseline for \"{id}\" is not cached, and '{}' is no longer in the built-in catalog.",
                    catalog.id
                ))
            })?;
            return plan_into_temp(adapter, &manifest, entry, id, None);
        }
        CapabilitySource::Git(git) => {
            let (guard, checkout, _) = match checkout {
                GitCheckout::Pinned => git::clone_to_temp(&git.url, Some(git.git_ref.as_str()))?,
                GitCheckout::Head => git::clone_to_temp(&git.url, None)?,
                GitCheckout::Tag(tag) => git::clone_tag_to_temp(&git.url, tag)?,
            };
            let dir = git::discover_capability(&checkout, &git.path, entry.capability_type)?;
            (Some(guard), dir)
        }
        CapabilitySource::Local(local) => {
            let dir = lockfile::absolutize(scope_root, Path::new(&local.path));
            if !dir.is_dir() {
                return Err(missing_local(&local.path));
            }
            (None, dir)
        }
        CapabilitySource::Pack(_) => unreachable!("pack members are materialized in place"),
    };
    let manifest = if capability_dir.join("tuff.toml").is_file() {
        load_manifest(&capability_dir)?
    } else {
        manifest::synthetic_manifest(&capability_dir, id, "materialized")?
    };
    plan_into_temp(adapter, &manifest, entry, id, source_guard)
}

/// Plan a manifest into a scratch tree and hand back the capability's
/// installed root within it, keeping any source checkout alive alongside.
fn plan_into_temp(
    adapter: AdapterKind,
    manifest: &manifest::CapabilityManifest,
    entry: &lockfile::CapabilityLockEntry,
    id: &str,
    source_guard: Option<TempDir>,
) -> Result<MaterializedTree> {
    let capability = resolve_capability(manifest)?;
    let temp = TempDir::new()?;
    let plans = adapter.plan(&capability, temp.path())?;
    let target_root = temp
        .path()
        .join(adapter.dir_prefix())
        .join(entry.capability_type.plural_dir())
        .join(id);
    for planned in plans {
        let path = temp.path().join(&planned.path);
        if path.starts_with(&target_root) {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, planned.content)?;
        }
    }
    let mut sources = vec![temp];
    if let Some(source_guard) = source_guard {
        sources.push(source_guard);
    }
    Ok(MaterializedTree {
        _sources: sources,
        path: target_root,
    })
}

fn installed_path(
    root: &Path,
    id: &str,
    entry: &lockfile::CapabilityLockEntry,
    target_entry: &lockfile::TargetLockEntry,
    target: &str,
) -> PathBuf {
    if !target_entry.installed_path.is_empty() {
        return root.join(&target_entry.installed_path);
    }
    let adapter = AdapterKind::from_id(target).unwrap_or(AdapterKind::OpenAgents);
    root.join(adapter.dir_prefix())
        .join(entry.capability_type.plural_dir())
        .join(id)
}

fn is_in_place_source_path(path: &str) -> bool {
    path.starts_with(".agents/") || path.starts_with(".claude/")
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

fn style_diff(diff: &str) -> String {
    if !super::use_color() {
        return diff.to_string();
    }
    diff.split_inclusive('\n')
        .map(|line| {
            let code = if line.starts_with("+++") || line.starts_with("---") {
                "36"
            } else if line.starts_with('+') {
                "32"
            } else if line.starts_with('-') {
                "31"
            } else {
                return line.to_string();
            };
            format!("\x1b[{code}m{line}\x1b[0m")
        })
        .collect()
}
