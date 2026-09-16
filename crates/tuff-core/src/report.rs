//! Dashboard reports (RFC-108): one JSON document describing one project at
//! one commit, which `tuff dashboard publish` sends to a dashboard server.
//!
//! A report is the project's lockfile plus what only the working tree can
//! say: whether the installed files still match (`tuff check`) and,
//! optionally, whether newer versions exist (`tuff outdated`). The schema
//! is a published format like the lockfile, and a server refuses a
//! `schema` it does not know.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, TuffError};
use crate::lockfile;

/// The report schema this Tuff writes and reads.
pub const REPORT_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub schema: u32,
    pub tuff_version: String,
    /// RFC 3339, UTC.
    pub generated_at: String,
    pub project: ProjectIdentity,
    /// `tuff.lock` in the current schema.
    pub lockfile: serde_json::Value,
    /// `tuff check --json` for the project scope.
    pub check: serde_json::Value,
    /// `tuff outdated --json` for the project scope, when asked for.
    pub outdated: Option<serde_json::Value>,
}

/// Which project a report describes (RFC-108 D3). `repository` and `path`
/// together identify it across reports; `name` is a label.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIdentity {
    pub repository: String,
    /// The project folder relative to the repository root, `.` for the root.
    pub path: String,
    pub name: String,
    pub commit: Option<String>,
    pub branch: Option<String>,
    /// Whether the project folder has uncommitted changes.
    pub dirty: bool,
}

/// Build the report for the project at `project_root`, whose `tuff.lock`
/// must exist. `project_name`, when given, replaces the repository value,
/// and is required outside git or without an `origin` remote. `outdated` is
/// computed by the caller, which owns the network.
pub fn build_report(
    project_root: &Path,
    project_name: Option<&str>,
    outdated: Option<serde_json::Value>,
) -> Result<Report> {
    let lock = lockfile::require_lockfile(project_root)?;
    let check = crate::check::run_checks(project_root, crate::check::CheckScope::Project)?;
    let lockfile: serde_json::Value = serde_json::from_str(&lockfile::render_lockfile(&lock)?)?;
    Ok(Report {
        schema: REPORT_SCHEMA,
        tuff_version: env!("CARGO_PKG_VERSION").to_string(),
        generated_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        project: project_identity(project_root, project_name)?,
        lockfile,
        check: serde_json::to_value(check)?,
        outdated,
    })
}

/// Work out where a project sits in its repository (RFC-108 D3).
pub fn project_identity(
    project_root: &Path,
    project_name: Option<&str>,
) -> Result<ProjectIdentity> {
    let project_root = project_root.canonicalize()?;
    let folder_name = |path: &Path| {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "project".to_string())
    };
    let missing_name = || {
        TuffError::usage(format!(
            "{} is not in a git repository with an 'origin' remote, so the report has no repository to name",
            project_root.display()
        ))
        .with_hint("pass --project <name> to name it")
    };

    let Ok(repo) = git2::Repository::discover(&project_root) else {
        let repository = project_name.ok_or_else(missing_name)?.to_string();
        return Ok(ProjectIdentity {
            repository,
            path: ".".to_string(),
            name: folder_name(&project_root),
            commit: None,
            branch: None,
            dirty: false,
        });
    };
    let workdir = repo
        .workdir()
        .ok_or_else(|| TuffError::usage("a bare git repository has no project folder"))?
        .canonicalize()?;
    let relative = project_root
        .strip_prefix(&workdir)
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let path = if relative.as_os_str().is_empty() {
        ".".to_string()
    } else {
        relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/")
    };

    let origin = repo
        .find_remote("origin")
        .ok()
        .and_then(|remote| remote.url().map(normalize_remote));
    let repository = match (project_name, origin) {
        (Some(name), _) => name.to_string(),
        (None, Some(origin)) => origin,
        (None, None) => return Err(missing_name()),
    };

    let head = repo.head().ok();
    let commit = head
        .as_ref()
        .and_then(|head| head.peel_to_commit().ok())
        .map(|commit| commit.id().to_string());
    let branch = head
        .as_ref()
        .filter(|head| head.is_branch())
        .and_then(|head| head.shorthand().map(str::to_string));

    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(false);
    if path != "." {
        options.pathspec(&path);
    }
    let dirty = repo
        .statuses(Some(&mut options))
        .map(|statuses| !statuses.is_empty())
        .unwrap_or(false);

    Ok(ProjectIdentity {
        repository,
        name: folder_name(&project_root),
        path,
        commit,
        branch,
        dirty,
    })
}

/// A git remote URL as one stable repository name: no scheme, credentials,
/// port, or trailing `.git`, and a lowercase host. `git@github.com:acme/x.git`
/// and `https://token@github.com/acme/x` both become `github.com/acme/x`.
/// A local path is kept, without a trailing `.git`.
pub fn normalize_remote(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    let trimmed = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    let (host, path) = if let Some((_, rest)) = trimmed.split_once("://") {
        match rest.split_once('/') {
            Some((authority, path)) => (authority, path),
            None => (rest, ""),
        }
    } else if !trimmed.starts_with('/')
        && !trimmed.starts_with('.')
        && let Some((authority, path)) = trimmed.split_once(':')
    {
        (authority, path)
    } else {
        return trimmed.to_string();
    };
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        host
    } else {
        format!("{host}/{path}")
    }
}

/// Every folder under `root` with a `tuff.lock`, `root` included, in path
/// order. `.git`, `node_modules`, `target`, and other hidden folders are
/// not searched.
pub fn find_projects(root: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        if lockfile::project_lockfile(&dir).is_file() {
            found.push(dir.clone());
        }
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }
            pending.push(entry.path());
        }
    }
    found.sort();
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remotes_normalise_to_one_name() {
        for url in [
            "git@github.com:acme/agents.git",
            "https://github.com/acme/agents.git",
            "https://x-access-token:secret@github.com/acme/agents",
            "ssh://git@GitHub.com:22/acme/agents.git/",
            "http://github.com/acme/agents",
        ] {
            assert_eq!(normalize_remote(url), "github.com/acme/agents", "{url}");
        }
        assert_eq!(normalize_remote("/srv/git/agents.git"), "/srv/git/agents");
        assert_eq!(normalize_remote("../agents"), "../agents");
    }

    #[test]
    fn projects_are_found_below_the_root_and_hidden_folders_are_skipped() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for dir in [
            "",
            "apps/support-agent",
            "apps/billing-agent",
            "node_modules/pkg",
            ".claude/nested",
            "target/debug",
        ] {
            let path = root.join(dir);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("tuff.lock"), "{}").unwrap();
        }
        std::fs::create_dir_all(root.join("apps/no-lock")).unwrap();
        let found: Vec<_> = find_projects(root)
            .unwrap()
            .into_iter()
            .map(|path| {
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(found, ["", "apps/billing-agent", "apps/support-agent"]);
    }

    #[test]
    fn a_project_outside_git_needs_a_name() {
        let temp = tempfile::tempdir().unwrap();
        let error = project_identity(temp.path(), None).unwrap_err();
        assert!(
            error.to_string().contains("--project")
                || error.hint().is_some_and(|hint| hint.contains("--project")),
            "{error}"
        );
        let identity = project_identity(temp.path(), Some("local-agents")).unwrap();
        assert_eq!(identity.repository, "local-agents");
        assert_eq!(identity.path, ".");
        assert!(identity.commit.is_none());
    }

    #[test]
    fn a_monorepo_project_is_named_by_its_origin_and_path() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let repo = git2::Repository::init(root).unwrap();
        repo.remote("origin", "git@github.com:acme/agents.git")
            .unwrap();
        let app = root.join("apps/billing-agent");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(app.join("README.md"), "x").unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_path(Path::new("apps/billing-agent/README.md"))
            .unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let signature = git2::Signature::now("t", "t@example.test").unwrap();
        let commit = repo
            .commit(Some("HEAD"), &signature, &signature, "init", &tree, &[])
            .unwrap();

        let identity = project_identity(&app, None).unwrap();
        assert_eq!(identity.repository, "github.com/acme/agents");
        assert_eq!(identity.path, "apps/billing-agent");
        assert_eq!(identity.name, "billing-agent");
        assert_eq!(
            identity.commit.as_deref(),
            Some(commit.to_string().as_str())
        );
        assert!(identity.branch.is_some());
        assert!(!identity.dirty);

        std::fs::write(app.join("notes.md"), "y").unwrap();
        assert!(project_identity(&app, None).unwrap().dirty);
        let root_identity = project_identity(root, None).unwrap();
        assert_eq!(root_identity.path, ".");
    }
}
