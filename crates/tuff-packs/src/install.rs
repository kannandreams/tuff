//! Install, update, and remove packs in a [`Workspace`], in two steps.
//!
//! A plan function does everything that can fail before a write: it checks
//! the request against the lockfile, renders every member into a staging
//! folder, and compares the result with the project. The returned [`Plan`]
//! lists each member and file that will change, and changes nothing.
//! [`apply`] then writes exactly that plan as one change, after checking
//! that nothing it touches has changed since the plan was made.
//!
//! ```no_run
//! use tuff_core::lockfile::{PackArtifactSource, PackSource};
//! use tuff_packs::install::{apply, plan_install};
//! use tuff_packs::workspace::{Baselines, Workspace};
//!
//! # fn main() -> tuff_packs::Result<()> {
//! let workspace = Workspace::open("/srv/workspaces/ws-42")?;
//! let artifact = tuff_core::pack::read_artifact("review.tuffpack".as_ref())?;
//! let source = PackSource::Artifact(PackArtifactSource { registry: None });
//! let plan = plan_install(&workspace, &artifact, &source, &["claude".to_string()])?;
//! for file in &plan.files {
//!     println!("{:?} {}", file.change, file.path);
//! }
//! apply(&workspace, &Baselines::none(), plan)?;
//! # Ok(())
//! # }
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tuff_core::adapter::{AgentAdapter, resolve_capability};
use tuff_core::error::{ErrorKind, Result, TuffError};
use tuff_core::lockfile::{self, PackLockEntry, PackProvenance, PackSource};
use tuff_core::manifest::{self, CapabilityType};
use tuff_core::pack::{self, PackArtifact, PackArtifactCapability};
use tuff_core::resolver::Scope;

use crate::adapter::AdapterKind;
use crate::build::canonical_target_ids;
use crate::engine::index::{self, CAPABILITY_INDEX_ID};
use crate::engine::{self, Destination, PolicyOptions};
use crate::preview::{artifact_target, dirty_pack_members};
use crate::workspace::{Baselines, Workspace};

/// What a plan does to a pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Install,
    Update,
    Remove,
}

/// A release of a pack: its version and artifact digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanRelease {
    pub version: String,
    pub digest: String,
}

/// What happens to one member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MemberChange {
    Add,
    Update,
    Remove,
}

/// One member of the pack and what the plan does to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanMember {
    pub id: String,
    #[serde(rename = "type")]
    pub capability_type: CapabilityType,
    /// The version it moves to; `None` for a member being removed.
    pub version: Option<String>,
    pub change: MemberChange,
}

/// What happens to one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileChange {
    /// The file does not exist yet.
    Create,
    /// The file exists and gets new contents.
    Modify,
    /// The file is removed.
    Delete,
}

/// One file in the workspace and what the plan does to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanFile {
    /// Relative to the workspace, with `/` separators.
    pub path: String,
    pub change: FileChange,
}

/// The readable part of a [`Plan`]: everything it will change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanSummary {
    pub operation: Operation,
    pub pack: String,
    /// The release installed now; `None` for an install.
    pub installed: Option<PlanRelease>,
    /// The release the plan installs; `None` for a removal.
    pub release: Option<PlanRelease>,
    /// Where the new release comes from, as `tuff.lock` will record it.
    pub source: Option<PackSource>,
    pub targets: Vec<String>,
    pub members: Vec<PlanMember>,
    /// Every file the plan writes or removes, `tuff.lock` included, sorted
    /// by path.
    pub files: Vec<PlanFile>,
    /// Notes about members, such as a hook that runs code. Show them before
    /// asking whether to apply.
    pub notes: Vec<String>,
}

/// A change to a workspace, worked out and not yet written. Read it through
/// [`PlanSummary`] (a `Plan` dereferences to one, and serializes as one),
/// then pass it to [`apply`].
///
/// A plan holds the rendered files in memory and a staging folder in the
/// system's temporary directory until it is applied or dropped.
pub struct Plan {
    summary: PlanSummary,
    work: Work,
}

struct Work {
    root: PathBuf,
    mutations: Vec<FileMutation>,
    stale: Vec<PathBuf>,
    /// Each touched path's content hash when the plan was made, `None` for
    /// a path that did not exist.
    expected: BTreeMap<PathBuf, Option<String>>,
    /// Baseline copies to keep once applied: tree hash and staged tree.
    baselines: Vec<(String, PathBuf)>,
    _staging: tempfile::TempDir,
}

impl Deref for Plan {
    type Target = PlanSummary;

    fn deref(&self) -> &PlanSummary {
        &self.summary
    }
}

impl Serialize for Plan {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        self.summary.serialize(serializer)
    }
}

impl std::fmt::Debug for Plan {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.summary.fmt(formatter)
    }
}

impl Plan {
    /// The readable part of the plan.
    pub fn summary(&self) -> &PlanSummary {
        &self.summary
    }

    fn new(mut summary: PlanSummary, root: &Path, staged: Staged) -> Result<Self> {
        let Staged {
            staging,
            mutations,
            stale,
            notes,
            baselines,
        } = staged;
        let mut expected = BTreeMap::new();
        let mut files = Vec::new();
        for path in mutations
            .iter()
            .map(|mutation| &mutation.path)
            .chain(&stale)
        {
            let hash = fs::read(path)
                .ok()
                .map(|bytes| lockfile::hash_bytes(&bytes));
            let change = if stale.contains(path) {
                FileChange::Delete
            } else if path.exists() {
                FileChange::Modify
            } else {
                FileChange::Create
            };
            files.push(PlanFile {
                path: lockfile::relative_or_absolute_fs(path, root).replace('\\', "/"),
                change,
            });
            expected.insert(path.clone(), hash);
        }
        files.sort_by(|left, right| left.path.cmp(&right.path));
        summary.files = files;
        summary.notes.extend(notes);
        Ok(Self {
            summary,
            work: Work {
                root: root.to_path_buf(),
                mutations,
                stale,
                expected,
                baselines,
                _staging: staging,
            },
        })
    }
}

/// A staged change, before it is described as a plan.
struct Staged {
    staging: tempfile::TempDir,
    mutations: Vec<FileMutation>,
    stale: Vec<PathBuf>,
    notes: Vec<String>,
    baselines: Vec<(String, PathBuf)>,
}

/// Plan installing every member of a verified artifact into the workspace
/// for `targets`, recording `source` as where the pack came from.
pub fn plan_install(
    workspace: &Workspace,
    artifact: &PackArtifact,
    source: &PackSource,
    targets: &[String],
) -> Result<Plan> {
    let repo_root = workspace.root();
    let target_ids = canonical_target_ids(targets)?;
    let current_lock = lockfile::require_lockfile(repo_root)?;
    if !current_lock
        .pack_members(&artifact.metadata.name)
        .is_empty()
    {
        return Err(TuffError::refused(format!(
            "pack {} is already installed",
            artifact.metadata.name
        ))
        .with_hint("use 'tuff update <member>' to move it to another release"));
    }
    preflight_pack_install(repo_root, artifact, &current_lock, &target_ids)?;
    let staging = tempfile::tempdir()?;
    let staged_lock_path = staging.path().join("tuff.lock");
    lockfile::write_lockfile_at(&staged_lock_path, &current_lock)?;
    copy_shared_configuration(repo_root, staging.path(), &target_ids)?;
    let mut notes = Vec::new();
    let mutations = stage_release(
        repo_root,
        staging.path(),
        &staged_lock_path,
        artifact,
        source,
        &target_ids,
        &mut notes,
    )?;
    let baselines = staged_baselines(staging.path(), &staged_lock_path, &target_ids)?;
    let summary = PlanSummary {
        operation: Operation::Install,
        pack: artifact.metadata.name.clone(),
        installed: None,
        release: Some(release_of(artifact)),
        source: Some(source.clone()),
        members: artifact
            .metadata
            .capabilities
            .iter()
            .map(|capability| PlanMember {
                id: capability.id.clone(),
                capability_type: capability.capability_type,
                version: Some(capability.version.clone()),
                change: MemberChange::Add,
            })
            .collect(),
        targets: target_ids,
        files: Vec::new(),
        notes: Vec::new(),
    };
    Plan::new(
        summary,
        repo_root,
        Staged {
            staging,
            mutations,
            stale: Vec::new(),
            notes,
            baselines,
        },
    )
}

/// What [`plan_update`] is asked to do.
#[derive(Debug, Clone, Copy)]
pub struct UpdateRequest<'a> {
    /// The installed pack's name.
    pub pack: &'a str,
    /// The release to move to.
    pub artifact: &'a PackArtifact,
    /// Where that release came from, as `tuff.lock` will record it.
    pub source: &'a PackSource,
    /// The harnesses to update. Empty means every harness the pack is
    /// installed for, which is also the only other accepted value.
    pub targets: &'a [String],
    /// Replace members with local edits, and accept an artifact that
    /// reuses the installed version with different content.
    pub force: bool,
}

/// Plan moving an installed pack to another release: members the release
/// drops are removed, new ones are added, and the rest are replaced.
/// Returns `None` when `artifact` is the installed release.
pub fn plan_update(workspace: &Workspace, request: UpdateRequest<'_>) -> Result<Option<Plan>> {
    let UpdateRequest {
        pack,
        artifact,
        source,
        targets,
        force,
    } = request;
    let repo_root = workspace.root();
    let lock = lockfile::require_lockfile(repo_root)?;
    let (members, provenance) = installed_pack(&lock, pack)?;
    if !is_new_release(&provenance, artifact, source, force)? {
        return Ok(None);
    }
    let target_ids = update_targets(&lock, &members, targets)?;
    for target in &target_ids {
        artifact_target(artifact, target)?;
    }
    let dirty = dirty_pack_members(repo_root, &lock, &members, &target_ids);
    if !dirty.is_empty() && !force {
        return Err(TuffError::drift(format!(
            "pack {pack} has local changes in {}",
            dirty.join(", ")
        ))
        .with_hint("run 'tuff diff <id>' first, or use --force to replace them"));
    }
    let staged = stage_replacement(
        repo_root,
        &lock,
        &members,
        Some((artifact, source)),
        &target_ids,
    )?;
    let mut planned = artifact
        .metadata
        .capabilities
        .iter()
        .map(|capability| PlanMember {
            id: capability.id.clone(),
            capability_type: capability.capability_type,
            version: Some(capability.version.clone()),
            change: if members.contains(&capability.id) {
                MemberChange::Update
            } else {
                MemberChange::Add
            },
        })
        .collect::<Vec<_>>();
    planned.extend(removed_members(&lock, &members, Some(artifact)));
    let summary = PlanSummary {
        operation: Operation::Update,
        pack: pack.to_string(),
        installed: Some(PlanRelease {
            version: provenance.version.clone(),
            digest: provenance.digest.clone(),
        }),
        release: Some(release_of(artifact)),
        source: Some(source.clone()),
        targets: target_ids,
        members: planned,
        files: Vec::new(),
        notes: Vec::new(),
    };
    Plan::new(summary, repo_root, staged).map(Some)
}

/// Plan removing every member of an installed pack.
pub fn plan_remove(workspace: &Workspace, pack: &str, force: bool) -> Result<Plan> {
    let repo_root = workspace.root();
    let lock = lockfile::require_lockfile(repo_root)?;
    let (members, provenance) = installed_pack(&lock, pack)?;
    let target_ids = installed_targets(&lock, &members);
    for member in &members {
        for target in &target_ids {
            let policies = policies_on_server_table(&lock, member, target);
            if let Some(policy) = policies.first() {
                return Err(TuffError::refused(format!(
                    "'{member}' has rules from policy '{policy}' on its table, so pack {pack} was not removed"
                ))
                .with_hint(format!(
                    "delete the policy first ('tuff delete {policy}'), or drop its rules for '{member}'"
                )));
            }
        }
    }
    let dirty = dirty_pack_members(repo_root, &lock, &members, &target_ids);
    if !dirty.is_empty() && !force {
        return Err(TuffError::drift(format!(
            "pack {pack} has local changes in {}",
            dirty.join(", ")
        ))
        .with_hint("run 'tuff diff <id>' first, or use --force to remove them"));
    }
    let staged = stage_replacement(repo_root, &lock, &members, None, &target_ids)?;
    let summary = PlanSummary {
        operation: Operation::Remove,
        pack: pack.to_string(),
        installed: Some(PlanRelease {
            version: provenance.version.clone(),
            digest: provenance.digest.clone(),
        }),
        release: None,
        source: None,
        targets: target_ids,
        members: removed_members(&lock, &members, None),
        files: Vec::new(),
        notes: Vec::new(),
    };
    Plan::new(summary, repo_root, staged)
}

/// Write `plan` to the workspace it was made for, as one change, and keep
/// its baseline copies in `baselines`.
///
/// Takes the workspace's lock, then refuses with a `drift` error when any
/// file the plan touches, `tuff.lock` included, changed since the plan was
/// made; make the plan again in that case. A failed write restores every
/// file already written.
pub fn apply(workspace: &Workspace, baselines: &Baselines, plan: Plan) -> Result<PlanSummary> {
    let Plan { mut summary, work } = plan;
    if !same_folder(&work.root, workspace.root()) {
        return Err(TuffError::usage(format!(
            "this plan was made for {}, not {}",
            work.root.display(),
            workspace.root().display()
        )));
    }
    let _lock = workspace.lock()?;
    for (path, expected) in &work.expected {
        let actual = fs::read(path)
            .ok()
            .map(|bytes| lockfile::hash_bytes(&bytes));
        if &actual != expected {
            return Err(TuffError::drift(format!(
                "{} changed after the plan was made",
                lockfile::relative_or_absolute_fs(path, &work.root)
            ))
            .with_hint("make the plan again"));
        }
    }
    commit_mutations(&work.mutations)?;
    for path in &work.stale {
        if path.is_file() {
            fs::remove_file(path)?;
        }
        remove_empty_parents(path, &work.root);
    }
    for (hash, tree) in &work.baselines {
        if let Err(error) = baselines.store(&work.root, hash, tree) {
            summary.notes.push(format!(
                "could not keep a baseline copy ({error}); 'tuff diff' names changed files but cannot show their original text"
            ));
        }
    }
    Ok(summary)
}

fn same_folder(left: &Path, right: &Path) -> bool {
    left == right
        || matches!(
            (left.canonicalize(), right.canonicalize()),
            (Ok(left), Ok(right)) if left == right
        )
}

/// Whether `artifact` is a release to move to from the installed one:
/// `Ok(false)` when it is the installed release. Refuses an artifact for
/// another pack, and without `force` an artifact that reuses the installed
/// version with different content.
pub fn is_new_release(
    installed: &PackProvenance,
    artifact: &PackArtifact,
    source: &PackSource,
    force: bool,
) -> Result<bool> {
    if artifact.metadata.name != installed.name {
        return Err(TuffError::refused(format!(
            "refusing to update pack {} from an artifact for pack {}",
            installed.name, artifact.metadata.name
        )));
    }
    // A pack built from git is rebuilt for every update, and its digest
    // changes whenever Tuff renders differently, so for git the commit
    // decides and this comparison applies to artifacts.
    if artifact.metadata.version == installed.version && matches!(source, PackSource::Artifact(_)) {
        if artifact.digest == installed.digest {
            return Ok(false);
        }
        if !force {
            return Err(TuffError::refused(format!(
                "pack {} {} is installed from sha256:{} but the artifact with the same version \
                 has sha256:{}; the version was republished with different content",
                installed.name, installed.version, installed.digest, artifact.digest
            ))
            .with_hint("use --force to replace it"));
        }
    }
    Ok(true)
}

/// An installed pack's members and the provenance they record.
fn installed_pack(lock: &lockfile::Lockfile, pack: &str) -> Result<(Vec<String>, PackProvenance)> {
    let members = lock.pack_members(pack);
    let provenance = members
        .first()
        .and_then(|member| lock.capabilities.get(member))
        .and_then(|entry| entry.source.as_pack())
        .cloned()
        .ok_or_else(|| {
            TuffError::not_found(format!("pack {pack} is not installed"))
                .with_hint("run 'tuff pack list' to see installed packs")
        })?;
    Ok((members, provenance))
}

/// Every harness any member of the pack is installed for.
fn installed_targets(lock: &lockfile::Lockfile, members: &[String]) -> Vec<String> {
    members
        .iter()
        .filter_map(|member| lock.capabilities.get(member))
        .flat_map(|entry| entry.targets.keys().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Installed members the new release (or a removal) leaves out.
fn removed_members(
    lock: &lockfile::Lockfile,
    members: &[String],
    release: Option<&PackArtifact>,
) -> Vec<PlanMember> {
    members
        .iter()
        .filter(|member| {
            release.is_none_or(|artifact| {
                !artifact
                    .metadata
                    .capabilities
                    .iter()
                    .any(|capability| &capability.id == *member)
            })
        })
        .filter_map(|member| {
            lock.capabilities.get(member).map(|entry| PlanMember {
                id: member.clone(),
                capability_type: entry.capability_type,
                version: None,
                change: MemberChange::Remove,
            })
        })
        .collect()
}

fn release_of(artifact: &PackArtifact) -> PlanRelease {
    PlanRelease {
        version: artifact.metadata.version.clone(),
        digest: artifact.digest.clone(),
    }
}

/// The baseline copies a staged change leaves to keep: every target of
/// every capability in the staged lockfile whose tree was rendered there.
fn staged_baselines(
    stage: &Path,
    staged_lock_path: &Path,
    target_ids: &[String],
) -> Result<Vec<(String, PathBuf)>> {
    let staged_lock = lockfile::read_lockfile_at(staged_lock_path)?;
    let mut baselines = Vec::new();
    for entry in staged_lock.capabilities.values() {
        for target in target_ids {
            let Some(target_entry) = entry.targets.get(target) else {
                continue;
            };
            if target_entry.installed_path.is_empty() {
                continue;
            }
            let tree = stage.join(&target_entry.installed_path);
            if tree.is_dir() {
                baselines.push((target_entry.sha256.clone(), tree));
            }
        }
    }
    Ok(baselines)
}

/// The installed policies with rules on an MCP server's own table for one
/// agent, as Codex's `disabled_tools` and tool `approval_mode` are. Removing
/// the server would take those rules with it.
pub fn policies_on_server_table(
    lock: &lockfile::Lockfile,
    server_id: &str,
    target: &str,
) -> Vec<String> {
    let Some(adapter) = AdapterKind::from_id(target) else {
        return Vec::new();
    };
    let config = adapter.mcp_config_relpath();
    if !tuff_core::policy::is_codex_config(config) {
        return Vec::new();
    }
    let prefix = format!("{server_id}:");
    lock.capabilities
        .iter()
        .filter(|(_, entry)| entry.capability_type == CapabilityType::Policy)
        .filter(|(_, entry)| {
            entry.targets.get(target).is_some_and(|target_entry| {
                target_entry.managed_permissions.iter().any(|permission| {
                    permission.settings_path == config && permission.rule.starts_with(&prefix)
                })
            })
        })
        .map(|(id, _)| id.clone())
        .collect()
}

/// Install a new release on top of a staging tree that holds the project's
/// lockfile and shared configuration, the way `tuff add pack` installs,
/// and return the writes that carry the result into the project. Each
/// member's install notes go into `notes`.
fn stage_release(
    repo_root: &Path,
    stage: &Path,
    staged_lock_path: &Path,
    artifact: &PackArtifact,
    source: &PackSource,
    target_ids: &[String],
    notes: &mut Vec<String>,
) -> Result<Vec<FileMutation>> {
    let staged_lock = lockfile::read_lockfile_at(staged_lock_path)?;
    for capability in &artifact.metadata.capabilities {
        if staged_lock.capabilities.contains_key(&capability.id) {
            return Err(TuffError::refused(format!(
                "capability '{}' in pack {} {} is already tracked from another source; pack update is all-or-nothing",
                capability.id, artifact.metadata.name, artifact.metadata.version
            )));
        }
    }
    let sources = stage.join("sources");
    pack::extract_prefix(artifact, "sources", &sources)?;
    // Baselines are kept by `apply`, from this staging tree, once the
    // change is written.
    let no_baselines = Baselines::none();
    let to = Destination {
        root: stage,
        scope: Scope::Project,
        baselines: &no_baselines,
    };
    for capability in &artifact.metadata.capabilities {
        let manifest = manifest::load_manifest(&sources.join(&capability.id))?;
        validate_artifact_member(capability, &manifest)?;
        notes.extend(manifest::install_notes(&manifest));
        let resolved = resolve_capability(&manifest)?;
        // The engine's own lines describe the staging tree; the plan
        // describes the change.
        let mut engine_messages = Vec::new();
        engine::install_capability(
            to,
            &resolved,
            &manifest,
            target_ids,
            None,
            PolicyOptions::default(),
            &mut engine_messages,
        )?;
    }
    validate_staged_target_hashes(stage, artifact, target_ids)?;

    let mut staged_lock = lockfile::read_lockfile_at(staged_lock_path)?;
    record_pack_entry(&mut staged_lock, artifact, source);
    let provenance = PackProvenance {
        name: artifact.metadata.name.clone(),
        version: artifact.metadata.version.clone(),
        digest: artifact.digest.clone(),
        registry: source.registry().map(str::to_string),
        path: String::new(),
    };
    for capability in &artifact.metadata.capabilities {
        let entry = staged_lock
            .capabilities
            .get_mut(&capability.id)
            .ok_or_else(|| {
                TuffError::new(format!(
                    "staged pack installation did not track '{}'",
                    capability.id
                ))
            })?;
        entry.source = lockfile::CapabilitySource::Pack(PackProvenance {
            path: capability.source_path.clone(),
            ..provenance.clone()
        });
        entry.version_scheme = entry.source.version_scheme_for(&entry.version);
    }
    lockfile::write_lockfile_at(staged_lock_path, &staged_lock)?;

    collect_install_mutations(repo_root, stage, artifact, &staged_lock, target_ids)
}

/// Replace one pack release with another, or with nothing, in a staging
/// tree, and work out the writes that carry the result into the project
/// plus the files the old release owned that nothing writes any more.
///
/// The staging tree starts from the project's lockfile and shared
/// configuration, the old members are removed from it exactly as
/// `tuff delete` would remove them (hook registrations and MCP entries
/// included), and the new release is installed on top through the same
/// path as `tuff add pack`. The project itself is not touched.
fn stage_replacement(
    repo_root: &Path,
    lock: &lockfile::Lockfile,
    members: &[String],
    release: Option<(&PackArtifact, &PackSource)>,
    target_ids: &[String],
) -> Result<Staged> {
    let staging = tempfile::tempdir()?;
    let staged_lock_path = staging.path().join("tuff.lock");
    lockfile::write_lockfile_at(&staged_lock_path, lock)?;
    let mut staged_lock = lockfile::read_lockfile_at(&staged_lock_path)?;

    // Paths the old release owns in the project. Each is either rewritten
    // by the new release (present in staging afterwards) or stale.
    let mut previous_paths = BTreeSet::<PathBuf>::new();
    for member in members {
        let Some(entry) = lock.capabilities.get(member) else {
            continue;
        };
        for target in target_ids {
            let Some(target_entry) = entry.targets.get(target) else {
                continue;
            };
            if !target_entry.installed_path.is_empty() {
                let tree = repo_root.join(&target_entry.installed_path);
                if tree.is_dir() {
                    collect_project_tree(repo_root, &tree, &mut previous_paths)?;
                }
            }
            for hook in &target_entry.managed_hooks {
                previous_paths.insert(PathBuf::from(&hook.settings_path));
            }
            if let Some(mcp_entry) = &target_entry.managed_mcp_entry {
                previous_paths.insert(PathBuf::from(&mcp_entry.config_path));
            }
        }
    }

    // The capability index is derived from the lockfile, so the old
    // release's index is owned by the old release too: when the new one
    // leaves nothing to index, the file must go rather than linger untracked.
    if let Some(index_entry) = lock.capabilities.get(CAPABILITY_INDEX_ID) {
        for target in target_ids {
            let Some(target_entry) = index_entry.targets.get(target) else {
                continue;
            };
            let tree = repo_root.join(&target_entry.installed_path);
            if !target_entry.installed_path.is_empty() && tree.is_dir() {
                collect_project_tree(repo_root, &tree, &mut previous_paths)?;
            }
        }
    }

    copy_shared_configuration(repo_root, staging.path(), target_ids)?;
    for member in members {
        let Some(entry) = lock.capabilities.get(member) else {
            continue;
        };
        for target in target_ids {
            let Some(target_entry) = entry.targets.get(target) else {
                continue;
            };
            let adapter = AdapterKind::from_id(target)
                .ok_or_else(|| TuffError::usage(format!("unknown harness '{target}'")))?;
            adapter.remove(member, staging.path(), &target_entry.managed_hooks)?;
        }
        staged_lock.capabilities.remove(member);
    }
    lockfile::write_lockfile_at(&staged_lock_path, &staged_lock)?;

    let mut notes = Vec::new();
    let mut mutations = match release {
        Some((artifact, source)) => stage_release(
            repo_root,
            staging.path(),
            &staged_lock_path,
            artifact,
            source,
            target_ids,
            &mut notes,
        )?,
        None => {
            // A removal: the index lists what is left, or goes.
            let no_baselines = Baselines::none();
            index::regenerate_capability_index(Destination {
                root: staging.path(),
                scope: Scope::Project,
                baselines: &no_baselines,
            })?;
            vec![FileMutation {
                path: repo_root.join("tuff.lock"),
                bytes: fs::read(&staged_lock_path)?,
                must_be_absent: false,
            }]
        }
    };
    // Files the old release emitted may legitimately exist in the project;
    // the new release overwrites them. Anything the old release owned that
    // the new one no longer writes is removed after the commit.
    let mut stale = Vec::new();
    for relative in &previous_paths {
        let destination = repo_root.join(relative);
        if let Some(mutation) = mutations
            .iter_mut()
            .find(|mutation| mutation.path == destination)
        {
            mutation.must_be_absent = false;
        } else if staging.path().join(relative).is_file() {
            mutations.push(FileMutation {
                path: destination,
                bytes: fs::read(staging.path().join(relative))?,
                must_be_absent: false,
            });
        } else {
            stale.push(destination);
        }
    }
    let baselines = staged_baselines(staging.path(), &staged_lock_path, target_ids)?;
    Ok(Staged {
        staging,
        mutations,
        stale,
        notes,
        baselines,
    })
}

fn record_pack_entry(lock: &mut lockfile::Lockfile, artifact: &PackArtifact, source: &PackSource) {
    lock.packs.insert(
        artifact.metadata.name.clone(),
        PackLockEntry {
            version: artifact.metadata.version.clone(),
            digest: artifact.digest.clone(),
            source: source.clone(),
            x: artifact.metadata.x.clone(),
        },
    );
}

fn preflight_pack_install(
    repo_root: &Path,
    artifact: &PackArtifact,
    lock: &lockfile::Lockfile,
    target_ids: &[String],
) -> Result<()> {
    for capability in &artifact.metadata.capabilities {
        if lock.capabilities.contains_key(&capability.id) {
            return Err(TuffError::refused(format!(
                "capability '{}' is already tracked",
                capability.id
            ))
            .with_hint(
                "pack installation is all-or-nothing; delete it first or choose another pack",
            ));
        }
    }
    for target_id in target_ids {
        let target = artifact_target(artifact, target_id)?;
        for capability in &target.capabilities {
            for path in &capability.emitted_files {
                let destination = repo_root.join(path);
                if destination.exists() {
                    return Err(TuffError::refused(format!(
                        "refusing to overwrite untracked file at {}",
                        destination.display()
                    ))
                    .with_hint("pack installation is all-or-nothing; move the file aside first"));
                }
            }
        }
    }
    Ok(())
}

fn copy_shared_configuration(source: &Path, destination: &Path, targets: &[String]) -> Result<()> {
    // The capability-index regeneration triggered by `install_capability`
    // reads `registered_adapters`, which reads this file's `agents` list.
    // Without it staged in advance, `read_config_at` would fall back to
    // writing a stray *default* config into the staging tree (debt #18)
    // and the index would regenerate against zero configured agents.
    let project_config = tuff_core::paths::project_config(source);
    if project_config.is_file() {
        fs::copy(
            &project_config,
            tuff_core::paths::project_config(destination),
        )?;
    }

    for target in targets {
        let adapter = AdapterKind::from_id(target)
            .ok_or_else(|| TuffError::usage(format!("unknown harness '{target}'")))?;
        for relative in [
            adapter.hook_settings_relpath(),
            adapter.mcp_config_relpath(),
        ] {
            let source_file = source.join(relative);
            if source_file.is_file() {
                let destination_file = destination.join(relative);
                if let Some(parent) = destination_file.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(source_file, destination_file)?;
            }
        }
    }
    Ok(())
}

fn validate_artifact_member(
    expected: &PackArtifactCapability,
    manifest: &manifest::CapabilityManifest,
) -> Result<()> {
    if manifest.id != expected.id
        || manifest.version != expected.version
        || manifest.capability_type != expected.capability_type
    {
        return Err(TuffError::corrupt(format!(
            "artifact source metadata does not match capability '{}'",
            expected.id
        )));
    }
    Ok(())
}

fn validate_staged_target_hashes(
    stage: &Path,
    artifact: &PackArtifact,
    targets: &[String],
) -> Result<()> {
    for target in targets {
        for capability in &artifact_target(artifact, target)?.capabilities {
            let actual = tuff_core::cache::hash_tree(&stage.join(&capability.installed_path))?;
            if actual != capability.sha256 {
                return Err(TuffError::corrupt(format!(
                    "rendered target hash mismatch for '{}' on '{}'",
                    capability.id, target
                )));
            }
        }
    }
    Ok(())
}

struct FileMutation {
    path: PathBuf,
    bytes: Vec<u8>,
    must_be_absent: bool,
}

fn collect_install_mutations(
    repo_root: &Path,
    stage: &Path,
    artifact: &PackArtifact,
    staged_lock: &lockfile::Lockfile,
    targets: &[String],
) -> Result<Vec<FileMutation>> {
    let mut paths = BTreeMap::<PathBuf, bool>::new();
    for target in targets {
        let adapter = AdapterKind::from_id(target)
            .ok_or_else(|| TuffError::usage(format!("unknown harness '{target}'")))?;
        for capability in &artifact.metadata.capabilities {
            let target_entry = staged_lock
                .capabilities
                .get(&capability.id)
                .and_then(|entry| entry.targets.get(adapter.id()))
                .ok_or_else(|| {
                    TuffError::new(format!(
                        "staged lockfile is missing '{}' for target '{}'",
                        capability.id,
                        adapter.id()
                    ))
                })?;
            collect_mutation_tree(stage, &stage.join(&target_entry.installed_path), &mut paths)?;
            for hook in &target_entry.managed_hooks {
                paths.insert(PathBuf::from(&hook.settings_path), false);
            }
        }

        // Installing this pack's capabilities regenerates the capability-
        // index skill inside the staging tree (see `install_capability`).
        // It isn't one of the pack's own declared capabilities, so the loop
        // above never walks it — without this, the staged lockfile would
        // reference index files that never made it to `repo_root`.
        if let Some(index_entry) = staged_lock.capabilities.get(CAPABILITY_INDEX_ID)
            && let Some(target_entry) = index_entry.targets.get(adapter.id())
        {
            collect_mutation_tree(stage, &stage.join(&target_entry.installed_path), &mut paths)?;
            // A project that already has a tool or MCP server
            // already has an index on disk, and it is tracked. The pack's
            // regenerated index replaces it; refusing to overwrite it would
            // make every such project unable to install a pack at all.
            let index_tracked_here = lockfile::require_lockfile(repo_root)
                .ok()
                .and_then(|current| {
                    current
                        .capabilities
                        .get(CAPABILITY_INDEX_ID)
                        .map(|entry| entry.targets.contains_key(adapter.id()))
                })
                .unwrap_or(false);
            if index_tracked_here {
                let index_root = PathBuf::from(&target_entry.installed_path);
                for (path, must_be_absent) in paths.iter_mut() {
                    if path.starts_with(&index_root) {
                        *must_be_absent = false;
                    }
                }
            }
        }

        let mcp_path = PathBuf::from(adapter.mcp_config_relpath());
        if stage.join(&mcp_path).is_file() {
            paths.insert(mcp_path, false);
        }
    }
    paths.insert(PathBuf::from("tuff.lock"), false);

    paths
        .into_iter()
        .map(|(relative, must_be_absent)| {
            let source = stage.join(&relative);
            Ok(FileMutation {
                path: repo_root.join(relative),
                bytes: fs::read(&source).map_err(|error| {
                    TuffError::new(format!(
                        "staged pack output is missing {}: {error}",
                        source.display()
                    ))
                })?,
                must_be_absent,
            })
        })
        .collect()
}

fn collect_mutation_tree(
    stage: &Path,
    current: &Path,
    paths: &mut BTreeMap<PathBuf, bool>,
) -> Result<()> {
    let mut entries = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(TuffError::refused(format!(
                "symbolic links are not allowed in staged pack output: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            collect_mutation_tree(stage, &path, paths)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(stage)
                .map_err(|error| TuffError::of(ErrorKind::Io, error.to_string()))?;
            paths.insert(relative.to_path_buf(), true);
        }
    }
    Ok(())
}

fn commit_mutations(mutations: &[FileMutation]) -> Result<()> {
    let mut created_directories = BTreeSet::new();
    for mutation in mutations {
        if mutation.must_be_absent && mutation.path.exists() {
            return Err(TuffError::refused(format!(
                "refusing to overwrite untracked file at {}",
                mutation.path.display()
            )));
        }
        if mutation.path.exists() && !mutation.path.is_file() {
            return Err(TuffError::refused(format!(
                "pack installation target is not a regular file: {}",
                mutation.path.display()
            )));
        }
        let mut parent = mutation.path.parent();
        while let Some(directory) = parent {
            if directory.exists() {
                break;
            }
            created_directories.insert(directory.to_path_buf());
            parent = directory.parent();
        }
    }
    let backups = mutations
        .iter()
        .map(|mutation| {
            let previous = if mutation.path.is_file() {
                Some(fs::read(&mutation.path)?)
            } else {
                None
            };
            Ok((mutation.path.clone(), previous))
        })
        .collect::<Result<Vec<_>>>()?;

    for mutation in mutations {
        if let Err(error) = atomic_write(&mutation.path, &mutation.bytes) {
            if let Err(rollback_error) = rollback_mutations(&backups, &created_directories) {
                return Err(TuffError::of(
                    ErrorKind::Io,
                    format!(
                        "pack installation failed while writing {}; rollback also failed: {rollback_error}",
                        mutation.path.display()
                    ),
                ));
            }
            return Err(TuffError::of(
                ErrorKind::Io,
                format!(
                    "pack installation failed while writing {}: {error}",
                    mutation.path.display()
                ),
            ));
        }
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| TuffError::usage("pack output path has no parent"))?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::Builder::new()
        .prefix("tuff-install-")
        .tempfile_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.flush()?;
    temporary
        .persist(path)
        .map_err(|error| TuffError::of(ErrorKind::Io, error.error.to_string()))?;
    Ok(())
}

fn rollback_mutations(
    backups: &[(PathBuf, Option<Vec<u8>>)],
    created_directories: &BTreeSet<PathBuf>,
) -> Result<()> {
    let mut first_error = None;
    for (path, previous) in backups.iter().rev() {
        let result = match previous {
            Some(bytes) => atomic_write(path, bytes),
            None => match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                    ) =>
                {
                    Ok(())
                }
                Err(error) => Err(error.into()),
            },
        };
        if let Err(error) = result
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    let mut directories = created_directories.iter().collect::<Vec<_>>();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        match fs::remove_dir(directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error.into());
                }
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// The agents a pack update applies to: every agent the pack is installed
/// for. A narrower `--harness` selection is refused rather than honoured,
/// because it would leave one agent's copy at the old release with a
/// lockfile that can only record one pack version per member.
pub fn update_targets(
    lock: &lockfile::Lockfile,
    members: &[String],
    requested: &[String],
) -> Result<Vec<String>> {
    let installed = members
        .iter()
        .filter_map(|member| lock.capabilities.get(member))
        .flat_map(|entry| entry.targets.keys().cloned())
        .collect::<BTreeSet<_>>();
    let installed = installed.into_iter().collect::<Vec<_>>();
    if requested.is_empty() {
        return Ok(installed);
    }
    let requested = canonical_target_ids(requested)?;
    let requested_set = requested.iter().collect::<BTreeSet<_>>();
    let installed_set = installed.iter().collect::<BTreeSet<_>>();
    if requested_set != installed_set {
        return Err(TuffError::usage(format!(
            "a pack update applies to every agent the pack is installed for ({})",
            installed.join(", ")
        ))
        .with_hint("drop --harness"));
    }
    Ok(installed)
}

/// Collect every regular file under `current`, relative to `repo_root`.
fn collect_project_tree(
    repo_root: &Path,
    current: &Path,
    paths: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let mut entries = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            collect_project_tree(repo_root, &path, paths)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(repo_root)
                .map_err(|error| TuffError::of(ErrorKind::Io, error.to_string()))?;
            paths.insert(relative.to_path_buf());
        }
    }
    Ok(())
}

/// Remove directories left empty by a stale-file removal, stopping at the
/// project root. Best effort: a directory that cannot be removed is left.
fn remove_empty_parents(path: &Path, repo_root: &Path) {
    let mut parent = path.parent();
    while let Some(directory) = parent {
        if directory == repo_root
            || !directory.starts_with(repo_root)
            || fs::read_dir(directory)
                .map(|mut entries| entries.next().is_some())
                .unwrap_or(true)
        {
            break;
        }
        if fs::remove_dir(directory).is_err() {
            break;
        }
        parent = directory.parent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_mutations_restores_previous_files_after_later_write_fails() {
        let temp = tempfile::tempdir().unwrap();
        let existing = temp.path().join("existing");
        let blocking_parent = temp.path().join("blocking-parent");
        fs::write(&existing, "before").unwrap();
        fs::write(&blocking_parent, "not a directory").unwrap();
        let mutations = vec![
            FileMutation {
                path: existing.clone(),
                bytes: b"after".to_vec(),
                must_be_absent: false,
            },
            FileMutation {
                path: blocking_parent.join("child"),
                bytes: b"unreachable".to_vec(),
                must_be_absent: false,
            },
        ];

        let error = commit_mutations(&mutations).unwrap_err();
        eprintln!("{error}");

        assert_eq!(fs::read_to_string(existing).unwrap(), "before");
    }
}
