//! `tuff scan` — find capabilities already sitting in a harness folder.
//!
//! Every other inventory command reads the lockfile, so a skill a developer
//! dropped into `.claude/skills/` by hand is invisible to Tuff no matter how
//! long it has been there: `list`, `status`, and `check` all say nothing
//! about it. `tuff add <path>` can adopt one in place if you already know
//! the path, which makes discovery — not adoption — the missing half.
//!
//! Scanning is read-only and works without a lockfile, so it can answer
//! "what would Tuff pick up here?" before a project is initialized.
//! `--adopt` hands each directory back to the same `add` code path, so scan
//! can never track something `add` would refuse.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::adapter::{AdapterKind, AgentAdapter};
use crate::error::{Result, TuffError};
use crate::lockfile;
use crate::manifest::{self, CapabilityType};
use crate::resolver::Scope;

use super::{infer_from_path, render_table, style_capability_type};

/// How deep below `<prefix>/<plural>/` a capability directory may sit.
///
/// One level is the documented layout. The extra levels exist because
/// harnesses do allow grouping directories — this repository's own
/// `.claude/skills/security/security-review/` is one — and a scan that
/// could not see them would quietly under-report. Past that, a deep tree is
/// a capability's own contents rather than more capabilities.
const MAX_DEPTH: usize = 3;

/// What Tuff already knows about a directory it found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    /// Recorded in the lockfile at this exact path.
    Tracked,
    /// Not recorded, and nothing else found shares its id.
    Untracked,
    /// Not recorded, and another untracked directory declares the same id.
    /// Adopting both would need one lockfile key to hold two paths, so the
    /// pair is reported and left for a person to resolve.
    Conflict,
    /// Not recorded, and missing something Tuff needs before it can be
    /// tracked at all. Predicted here rather than discovered halfway
    /// through `--adopt`, so the report says what to fix.
    Blocked,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Self::Tracked => "tracked",
            Self::Untracked => "untracked",
            Self::Conflict => "conflict",
            Self::Blocked => "blocked",
        }
    }
}

/// One capability directory found on disk.
struct Found {
    id: String,
    capability_type: CapabilityType,
    /// What the source declares, if anything. A capability with no declared
    /// version is not an error: `add` records `0.1.0` for it.
    version: Option<String>,
    description: Option<String>,
    /// The harness whose layout this path belongs to, as `add` infers it.
    agent: String,
    /// Relative to the repository root, with forward slashes.
    path: String,
    dir: PathBuf,
    status: Status,
    /// Why this cannot be adopted, when that is knowable up front.
    reason: Option<String>,
}

pub fn cmd_scan(repo_root: &Path, adopt: bool, paths: &[PathBuf], json: bool) -> Result<()> {
    if !adopt && !paths.is_empty() {
        return Err(
            TuffError::usage("paths select what to adopt, so they need --adopt")
                .with_hint("run 'tuff scan' with no arguments to see what is there"),
        );
    }

    let tracked = tracked_paths(repo_root)?;
    let found = discover(repo_root, tracked.as_ref())?;

    if adopt {
        return adopt_found(repo_root, &found, paths);
    }
    if json {
        return report_json(&found, tracked.is_none());
    }
    report_table(repo_root, &found, tracked.is_none())
}

/// Every installed path the project lockfile records, or `None` when there
/// is no lockfile at all.
///
/// The distinction matters: an empty set means an initialized project that
/// tracks nothing, while `None` means Tuff has never been run here, and the
/// two want different closing advice.
fn tracked_paths(repo_root: &Path) -> Result<Option<BTreeSet<String>>> {
    if !lockfile::project_lockfile(repo_root).exists() {
        return Ok(None);
    }
    let lockfile = lockfile::require_scoped_lockfile(repo_root, Scope::Project)?;
    let mut paths = BTreeSet::new();
    for entry in lockfile.capabilities.values() {
        for target in entry.targets.values() {
            if !target.installed_path.is_empty() {
                paths.insert(normalize(&target.installed_path));
            }
        }
    }
    Ok(Some(paths))
}

fn normalize(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_string()
}

/// Walk each detected harness folder and describe every capability
/// directory in it.
///
/// Prefixes are deduplicated before walking. `.agents` is the `dir_prefix`
/// of both the Codex and Open Agents adapters, so walking per adapter would
/// report every capability there twice under two harness names; walking per
/// directory reports it once, and `infer_from_path` names the harness the
/// same way `add` would.
fn discover(repo_root: &Path, tracked: Option<&BTreeSet<String>>) -> Result<Vec<Found>> {
    let mut prefixes: BTreeSet<&'static str> = BTreeSet::new();
    for adapter in AdapterKind::all() {
        if adapter.detect(repo_root) {
            prefixes.insert(adapter.dir_prefix());
        }
    }

    let mut found = Vec::new();
    for prefix in prefixes {
        for capability_type in KINDS {
            let root = repo_root.join(prefix).join(capability_type.plural_dir());
            if !root.is_dir() {
                continue;
            }
            let mut dirs = Vec::new();
            collect_dirs(&root, 1, &mut dirs)?;
            for dir in dirs {
                found.push(describe(repo_root, &dir, *capability_type, tracked));
            }
        }
    }

    found.sort_by(|left, right| left.path.cmp(&right.path));
    mark_conflicts(&mut found);
    Ok(found)
}

/// The capability kinds a harness folder can hold. `policy` is missing on
/// purpose: no adapter emits one, so no harness folder can contain one.
const KINDS: &[CapabilityType] = &[
    CapabilityType::Skill,
    CapabilityType::Tool,
    CapabilityType::Hook,
    CapabilityType::Workflow,
    CapabilityType::McpServer,
];

/// Collect the capability directories under one `<prefix>/<plural>/` root.
///
/// A directory holding at least one file is a capability, and its
/// subdirectories are its own contents rather than more capabilities, so
/// the walk stops there. A directory holding only directories is a grouping
/// folder and is descended into. That rule matches what `add` accepts: a
/// directory with no files of its own is exactly what it rejects as
/// "no source files found".
fn collect_dirs(root: &Path, depth: usize, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut children = Vec::new();
    let mut has_file = false;
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            children.push(entry.path());
        } else {
            has_file = true;
        }
    }

    if has_file {
        out.push(root.to_path_buf());
        return Ok(());
    }
    if depth >= MAX_DEPTH {
        return Ok(());
    }
    children.sort();
    for child in children {
        collect_dirs(&child, depth + 1, out)?;
    }
    Ok(())
}

/// Read what a directory says about itself, without loading it as a
/// capability. A malformed `tuff.toml` must not stop the scan: reporting
/// the directory with the name it has on disk is more useful than failing
/// the whole command over one bad file, and `--adopt` will surface the
/// parse error when it tries.
fn describe(
    repo_root: &Path,
    dir: &Path,
    fallback_type: CapabilityType,
    tracked: Option<&BTreeSet<String>>,
) -> Found {
    let manifest = dir
        .join("tuff.toml")
        .is_file()
        .then(|| manifest::load_manifest(dir).ok())
        .flatten();

    let id = manifest.as_ref().map(|m| m.id.clone()).unwrap_or_else(|| {
        dir.file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default()
    });
    let capability_type = manifest
        .as_ref()
        .map(|m| m.capability_type)
        .unwrap_or(fallback_type);

    let path = lockfile::relative_or_absolute_fs(dir, repo_root).replace('\\', "/");
    let reason = missing_declaration(capability_type, manifest.as_ref());
    let status = match tracked {
        Some(tracked) if tracked.contains(&path) => Status::Tracked,
        _ if reason.is_some() => Status::Blocked,
        _ => Status::Untracked,
    };

    Found {
        id,
        capability_type,
        version: manifest::declared_version(dir),
        description: manifest::declared_description(dir).filter(|text| !text.is_empty()),
        agent: infer_from_path(dir).1,
        path,
        dir: dir.to_path_buf(),
        status,
        reason,
    }
}

/// What a directory is missing before `add` could resolve it, if anything.
///
/// Only a skill is self-describing: every other kind carries configuration
/// that lives nowhere but a `tuff.toml`, and `resolve_capability` refuses
/// without it. Checking the same conditions here turns a failure halfway
/// through `--adopt` into a line in the report that says what to write.
fn missing_declaration(
    capability_type: CapabilityType,
    manifest: Option<&manifest::CapabilityManifest>,
) -> Option<String> {
    // Phrased to follow "it", so the same words serve the report line and
    // the error from naming a blocked path explicitly.
    let missing = |section: &str| Some(format!("is missing the [{section}] section in tuff.toml"));
    match capability_type {
        CapabilityType::Skill => None,
        CapabilityType::Tool => match manifest {
            Some(m) => match (m.parameters.is_some(), m.implementation.is_some()) {
                (true, true) => None,
                (true, false) => missing("implementation"),
                (false, true) => missing("parameters"),
                (false, false) => Some(
                    "is missing the [parameters] and [implementation] sections in tuff.toml"
                        .to_string(),
                ),
            },
            None => Some(
                "has no tuff.toml, and a tool needs [parameters] and [implementation]".to_string(),
            ),
        },
        CapabilityType::Hook => match manifest {
            Some(m) if m.hook.is_some() => None,
            // A hook registered in the harness's own settings file has no
            // `[hook]` section to find, and adopting it needs the fragment
            // to merge, which only a person can point at.
            _ => Some(
                "is missing the [hook] section in tuff.toml; adopt a native hook with \
                 'tuff add hook <path> --hook-file <fragment>'"
                    .to_string(),
            ),
        },
        CapabilityType::Workflow => match manifest {
            Some(m) if m.workflow.is_some() => None,
            _ => missing("workflow"),
        },
        CapabilityType::McpServer => match manifest {
            Some(m) if m.server.is_some() => None,
            _ => missing("server"),
        },
        CapabilityType::Policy => Some("cannot be installed yet".to_string()),
    }
}

/// Flag ids claimed by more than one untracked directory.
///
/// The lockfile is keyed by id, so two directories declaring the same one
/// cannot both be adopted. Reporting the pair is the honest answer;
/// silently adopting whichever sorted first would hide a duplicate the
/// developer probably wants to know about.
fn mark_conflicts(found: &mut [Found]) {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for entry in found.iter() {
        if entry.status == Status::Untracked {
            *counts.entry(entry.id.as_str()).or_default() += 1;
        }
    }
    let duplicated: BTreeSet<String> = counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(id, _)| id.to_string())
        .collect();
    for entry in found.iter_mut() {
        if entry.status == Status::Untracked && duplicated.contains(&entry.id) {
            entry.status = Status::Conflict;
        }
    }
}

fn report_json(found: &[Found], uninitialized: bool) -> Result<()> {
    let rows: Vec<serde_json::Value> = found
        .iter()
        .map(|entry| {
            serde_json::json!({
                "id": entry.id,
                "type": entry.capability_type,
                "version": entry.version,
                "description": entry.description,
                "agent": entry.agent,
                "path": entry.path,
                "status": entry.status.as_str(),
                "reason": entry.reason,
                // A project with no lockfile can be scanned but not tracked,
                // so a caller can tell "run init first" from "adopt this".
                "initialized": !uninitialized,
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&rows)?);
    Ok(())
}

fn report_table(repo_root: &Path, found: &[Found], uninitialized: bool) -> Result<()> {
    if found.is_empty() {
        println!("no capabilities found in {}", searched(repo_root));
        return Ok(());
    }

    let rows: Vec<Vec<String>> = found
        .iter()
        .map(|entry| {
            vec![
                entry.path.clone(),
                entry.id.clone(),
                style_capability_type(entry.capability_type),
                entry.version.clone().unwrap_or_else(|| "—".to_string()),
                entry.agent.clone(),
                style_scan_status(entry.status),
            ]
        })
        .collect();

    println!(
        "{}",
        render_table(&["PATH", "ID", "TYPE", "VERSION", "AGENT", "STATUS"], &rows)
    );

    let untracked = count(found, Status::Untracked);
    let conflicts = count(found, Status::Conflict);
    let tracked = count(found, Status::Tracked);

    if tracked > 0 {
        println!("{tracked} already tracked");
    }
    if conflicts > 0 {
        println!(
            "{conflicts} share an id with another directory; \
             adopt one explicitly with 'tuff add <path> --name <name>'"
        );
    }
    for entry in found.iter().filter(|entry| entry.status == Status::Blocked) {
        if let Some(reason) = &entry.reason {
            println!("{} {}", entry.path, reason);
        }
    }
    if untracked == 0 {
        return Ok(());
    }
    if uninitialized {
        println!("{untracked} untracked; run 'tuff init' first, then 'tuff scan --adopt'");
    } else {
        println!("{untracked} untracked; track them with 'tuff scan --adopt'");
    }
    Ok(())
}

fn searched(repo_root: &Path) -> String {
    let mut prefixes: BTreeSet<&'static str> = BTreeSet::new();
    for adapter in AdapterKind::all() {
        if adapter.detect(repo_root) {
            prefixes.insert(adapter.dir_prefix());
        }
    }
    if prefixes.is_empty() {
        return "this project (no harness folder found)".to_string();
    }
    prefixes.into_iter().collect::<Vec<_>>().join(", ")
}

fn count(found: &[Found], status: Status) -> usize {
    found.iter().filter(|entry| entry.status == status).count()
}

fn style_scan_status(status: Status) -> String {
    match status {
        Status::Tracked => format!(
            "{} {}",
            super::paint("✓", "32"),
            super::paint("tracked", "32")
        ),
        Status::Untracked => format!(
            "{} {}",
            super::paint("+", "36"),
            super::paint("untracked", "36")
        ),
        Status::Conflict => format!(
            "{} {}",
            super::paint("!", "33"),
            super::paint("conflict", "33")
        ),
        // Deliberately not red: nothing is broken, Tuff just cannot track
        // this until someone writes down what it is.
        Status::Blocked => format!(
            "{} {}",
            super::paint("·", "2"),
            super::paint("blocked", "2")
        ),
    }
}

/// Track what the scan found, by handing each directory to `add`.
///
/// Adoption runs through `cmd_add_local` rather than writing lockfile rows
/// here, so everything `add` does — the collision warning, the baseline
/// hash, the cache population, the `Imported` ownership that keeps `delete`
/// from removing files Tuff did not write — happens exactly once, in one
/// place.
fn adopt_found(repo_root: &Path, found: &[Found], selected: &[PathBuf]) -> Result<()> {
    if !lockfile::project_lockfile(repo_root).exists() {
        return Err(TuffError::not_found(format!(
            "{} is missing",
            lockfile::relative_or_absolute_fs(&lockfile::project_lockfile(repo_root), repo_root)
        ))
        .with_hint("run 'tuff init' first; 'tuff scan' alone needs no project"));
    }

    let wanted = selection(repo_root, found, selected)?;
    if wanted.is_empty() {
        println!("nothing to adopt");
        return Ok(());
    }

    let mut adopted = 0usize;
    let mut failed = 0usize;
    for entry in wanted {
        match super::cmd_add_local_path(repo_root, &entry.dir, &entry.agent, entry.capability_type)
        {
            Ok(()) => adopted += 1,
            Err(error) => {
                failed += 1;
                eprintln!("could not adopt {}: {}", entry.path, error);
            }
        }
    }

    if adopted > 0 {
        println!(
            "adopted {adopted} {}",
            if adopted == 1 {
                "capability"
            } else {
                "capabilities"
            }
        );
        // Said once, at the moment it becomes true. An adopted capability
        // was already on disk when Tuff met it, so there is nowhere to
        // check for a newer version and `outdated` will keep reporting it
        // as unchecked forever. That is honest, not a defect, but it is
        // surprising if nobody says so.
        println!("these have no upstream source, so 'tuff outdated' cannot check them for updates");
    }
    if failed > 0 {
        return Err(TuffError::refused(format!(
            "{failed} of {} could not be adopted",
            adopted + failed
        )));
    }
    Ok(())
}

/// The entries `--adopt` should act on: everything untracked, or just the
/// paths named on the command line.
///
/// A named path that the scan did not find is an error rather than a
/// silent skip, because the caller believes it exists; naming a tracked or
/// conflicting one is refused for the reason it is not adoptable.
fn selection<'a>(
    repo_root: &Path,
    found: &'a [Found],
    selected: &[PathBuf],
) -> Result<Vec<&'a Found>> {
    if selected.is_empty() {
        return Ok(found
            .iter()
            .filter(|entry| entry.status == Status::Untracked)
            .collect());
    }

    let mut wanted = Vec::new();
    for path in selected {
        let absolute = lockfile::absolutize(repo_root, path);
        let entry = found
            .iter()
            .find(|entry| same_dir(&entry.dir, &absolute))
            .ok_or_else(|| {
                TuffError::not_found(format!(
                    "'{}' is not a capability directory this scan found",
                    path.display()
                ))
                .with_hint("run 'tuff scan' to see the paths it can adopt")
            })?;
        match entry.status {
            Status::Untracked => wanted.push(entry),
            Status::Tracked => {
                return Err(TuffError::refused(format!(
                    "'{}' is already tracked",
                    entry.path
                )));
            }
            Status::Conflict => {
                return Err(TuffError::refused(format!(
                    "'{}' shares the id '{}' with another directory",
                    entry.path, entry.id
                ))
                .with_hint("adopt one explicitly with 'tuff add <path> --name <name>'"));
            }
            Status::Blocked => {
                let reason = entry.reason.clone().unwrap_or_default();
                return Err(TuffError::refused(format!(
                    "'{}' cannot be tracked as it stands: it {reason}",
                    entry.path
                )));
            }
        }
    }
    Ok(wanted)
}

fn same_dir(left: &Path, right: &Path) -> bool {
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn skill(root: &Path, relative: &str, body: &str) {
        let dir = root.join(relative);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), body).unwrap();
    }

    #[test]
    fn a_grouping_directory_is_descended_into_but_not_reported() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        skill(root, ".claude/skills/plain", "# plain\n");
        skill(root, ".claude/skills/group/nested", "# nested\n");

        let found = discover(root, None).unwrap();
        let paths: Vec<&str> = found.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![".claude/skills/group/nested", ".claude/skills/plain"]
        );
    }

    #[test]
    fn one_id_at_two_paths_is_a_conflict_rather_than_a_pick() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        skill(root, ".claude/skills/review", "# review\n");
        skill(root, ".claude/skills/group/review", "# review\n");

        let found = discover(root, None).unwrap();
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|entry| entry.status == Status::Conflict));
    }

    #[test]
    fn a_shared_prefix_is_walked_once() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        // `.agents` is the dir_prefix of both Codex and Open Agents.
        skill(root, ".agents/skills/shared", "# shared\n");

        let found = discover(root, None).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].agent, "open-agents");
    }

    #[test]
    fn a_tracked_path_is_reported_as_tracked() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        skill(root, ".claude/skills/known", "# known\n");

        let mut tracked = BTreeSet::new();
        tracked.insert(".claude/skills/known".to_string());
        let found = discover(root, Some(&tracked)).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].status, Status::Tracked);
    }

    #[test]
    fn frontmatter_supplies_the_version_and_description() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        skill(
            root,
            ".claude/skills/described",
            "---\nname: described\nversion: 2.1.0\ndescription: Reviews a diff.\n---\n\n# D\n",
        );

        let found = discover(root, None).unwrap();
        assert_eq!(found[0].version.as_deref(), Some("2.1.0"));
        assert_eq!(found[0].description.as_deref(), Some("Reviews a diff."));
    }

    #[test]
    fn kinds_are_read_from_the_directory_they_sit_in() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join(".claude/hooks/gate")).unwrap();
        fs::write(root.join(".claude/hooks/gate/run.sh"), "echo ok\n").unwrap();

        let found = discover(root, None).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].capability_type, CapabilityType::Hook);
    }
}
