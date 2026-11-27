//! Previewing a pack update (RFC-110 D5) and finding local edits that
//! block one. Nothing here writes to the workspace.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use tuff_core::error::{Result, TuffError};
use tuff_core::lockfile::{self, PackProvenance, PackSource};
use tuff_core::pack::{self, PackArtifact, PackArtifactTarget};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePreview {
    pub schema: u8,
    pub pack: String,
    /// `update`: there is a release to apply.
    pub status: &'static str,
    pub from: PreviewRelease,
    pub to: PreviewRelease,
    pub targets: Vec<String>,
    pub members: Vec<PreviewMember>,
    /// Members with local edits, as `id (harness)`. Applying the update
    /// replaces those edits.
    pub blocked_by: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct PreviewRelease {
    pub version: String,
    pub digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<PackSource>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewMember {
    pub id: String,
    /// `added`, `updated`, `removed`, or `unchanged`.
    pub change: &'static str,
    pub from_version: Option<String>,
    pub to_version: Option<String>,
    pub files: Vec<PreviewFile>,
}

#[derive(serde::Serialize)]
pub struct PreviewFile {
    pub target: String,
    pub path: String,
    pub status: String,
    /// The unified diff of this file, empty for a binary file.
    pub patch: String,
}

/// The preview behind `tuff update <member> --check --json` (RFC-110 D5):
/// every member the new release adds, updates, or removes, and for each the
/// files that change on each harness, compared with what is on disk under
/// `repo_root` now. `members` are the pack's installed members, `artifact`
/// the new release built for `target_ids`, and `new_source` where it came
/// from. Nothing is written.
pub fn preview_update(
    repo_root: &Path,
    lock: &lockfile::Lockfile,
    provenance: &PackProvenance,
    members: &[String],
    artifact: &PackArtifact,
    new_source: &PackSource,
    target_ids: &[String],
) -> Result<UpdatePreview> {
    let scratch = tempfile::tempdir()?;
    let empty = scratch.path().join("empty");
    fs::create_dir_all(&empty)?;
    let mut rendered = BTreeMap::new();
    for target in target_ids {
        let output = scratch.path().join("targets").join(target);
        pack::extract_prefix(artifact, &format!("targets/{target}"), &output)?;
        rendered.insert(target.clone(), output);
    }

    let mut ids = members.iter().cloned().collect::<BTreeSet<_>>();
    ids.extend(
        artifact
            .metadata
            .capabilities
            .iter()
            .map(|capability| capability.id.clone()),
    );
    let mut preview_members = Vec::new();
    for id in ids {
        let installed = members
            .iter()
            .any(|member| member == &id)
            .then(|| lock.capabilities.get(&id))
            .flatten();
        let incoming = artifact
            .metadata
            .capabilities
            .iter()
            .find(|capability| capability.id == id);
        let mut files = Vec::new();
        for target in target_ids {
            let old_relative = installed
                .and_then(|entry| entry.targets.get(target))
                .map(|entry| entry.installed_path.clone());
            let new_relative = incoming
                .and_then(|_| artifact_target(artifact, target).ok())
                .and_then(|rendered| rendered.capabilities.iter().find(|c| c.id == id))
                .map(|capability| capability.installed_path.clone());
            let old_root = old_relative
                .as_ref()
                .map(|relative| repo_root.join(relative))
                .filter(|path| path.is_dir())
                .unwrap_or_else(|| empty.clone());
            let new_root = new_relative
                .as_ref()
                .map(|relative| rendered[target].join(relative))
                .filter(|path| path.is_dir())
                .unwrap_or_else(|| empty.clone());
            let shown = new_relative.or(old_relative).unwrap_or_default();
            let comparison = tuff_core::tree_diff::compare(&old_root, &new_root)?;
            let patches = split_patch(&comparison.patch);
            for change in comparison.changes {
                files.push(PreviewFile {
                    target: target.clone(),
                    path: if shown.is_empty() {
                        change.path.clone()
                    } else {
                        format!("{shown}/{}", change.path)
                    },
                    status: change.status,
                    patch: patches.get(&change.path).cloned().unwrap_or_default(),
                });
            }
        }
        let change = match (installed, incoming) {
            (None, Some(_)) => "added",
            (Some(_), None) => "removed",
            _ if files.is_empty() => "unchanged",
            _ => "updated",
        };
        preview_members.push(PreviewMember {
            id,
            change,
            from_version: installed.map(|entry| entry.version.clone()),
            to_version: incoming.map(|capability| capability.version.clone()),
            files,
        });
    }

    let installed_source = lock
        .normalized_packs()
        .get(&provenance.name)
        .map(|pack| pack.source.clone());
    Ok(UpdatePreview {
        schema: 1,
        pack: provenance.name.clone(),
        status: "update",
        from: PreviewRelease {
            version: provenance.version.clone(),
            digest: provenance.digest.clone(),
            source: installed_source,
        },
        to: PreviewRelease {
            version: artifact.metadata.version.clone(),
            digest: artifact.digest.clone(),
            source: Some(new_source.clone()),
        },
        targets: target_ids.to_vec(),
        members: preview_members,
        blocked_by: dirty_pack_members(repo_root, lock, members, target_ids),
    })
}

/// Split a multi-file unified diff into one patch per file, keyed by the
/// path after `b/` (or `a/` for a deleted file).
fn split_patch(patch: &str) -> BTreeMap<String, String> {
    let mut patches = BTreeMap::new();
    let mut current: Option<(String, String)> = None;
    for line in patch.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some((path, body)) = current.take() {
                patches.insert(path, body);
            }
            let path = rest
                .trim_end()
                .rsplit_once(" b/")
                .map(|(_, path)| path.to_string())
                .unwrap_or_default();
            current = Some((path, line.to_string()));
        } else if let Some((_, body)) = current.as_mut() {
            body.push_str(line);
        }
    }
    if let Some((path, body)) = current {
        patches.insert(path, body);
    }
    patches
}

/// The installed members with local edits on any of `target_ids`, as
/// `id (harness)`.
pub fn dirty_pack_members(
    repo_root: &Path,
    lock: &lockfile::Lockfile,
    members: &[String],
    target_ids: &[String],
) -> Vec<String> {
    let mut dirty = Vec::new();
    for member in members {
        let Some(entry) = lock.capabilities.get(member) else {
            continue;
        };
        for target in target_ids {
            if let Some(target_entry) = entry.targets.get(target)
                && local_modifications(repo_root, member, target_entry).any()
            {
                dirty.push(format!("{member} ({target})"));
            }
        }
    }
    dirty
}

/// Where a tracked target has drifted from its recorded baseline.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalModifications {
    /// The installed tree or an emitted file differs from baseline.
    pub files: bool,
    /// A managed hook registration or MCP entry differs from baseline.
    pub managed: bool,
}

impl LocalModifications {
    pub fn any(self) -> bool {
        self.files || self.managed
    }
}

/// Check one tracked target under `scope_root` for local edits, the same
/// way removing a capability and updating a pack decide whether `--force`
/// is needed. It compares with the hashes in the lockfile.
pub fn local_modifications(
    scope_root: &Path,
    id: &str,
    target_entry: &lockfile::TargetLockEntry,
) -> LocalModifications {
    let modified_tree = !target_entry.installed_path.is_empty()
        && tuff_core::cache::hash_tree(&scope_root.join(&target_entry.installed_path))
            .map(|hash| hash != target_entry.sha256)
            .unwrap_or(true);
    let files = modified_tree;
    let managed = target_entry
        .managed_hooks
        .iter()
        .any(|hook| lockfile::managed_hook_status(scope_root, hook) == "modified")
        || target_entry.managed_permissions.iter().any(|permission| {
            tuff_core::policy::managed_permission_status(scope_root, permission) != "clean"
        })
        || target_entry
            .managed_mcp_entry
            .as_ref()
            .is_some_and(|entry| {
                lockfile::managed_mcp_entry_status(scope_root, id, entry) == "modified"
            });
    LocalModifications { files, managed }
}

/// The target `target` of an artifact.
pub fn artifact_target<'a>(
    artifact: &'a PackArtifact,
    target: &str,
) -> Result<&'a PackArtifactTarget> {
    artifact
        .metadata
        .targets
        .iter()
        .find(|item| item.id == target)
        .ok_or_else(|| {
            TuffError::usage(format!(
                "pack '{}' has no target '{}'",
                artifact.metadata.name, target
            ))
        })
}
