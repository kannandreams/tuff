use std::{
    path::{Path, PathBuf},
    process::Command,
};

use tempfile::TempDir;
use url::Url;

use crate::error::{Result, TuffError};
use crate::manifest::CapabilityType;

pub fn is_git_url(s: &str) -> bool {
    s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("git@")
        || s.starts_with("file://")
}

fn clean_git_url(raw: &str) -> (String, Option<String>) {
    let parsed = match Url::parse(raw) {
        Ok(u) => u,
        Err(_) => return (raw.to_string(), None),
    };

    let host = parsed.host_str().unwrap_or("");
    let path = parsed.path();
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();

    if (host == "github.com" || host.ends_with(".github.com"))
        && segments.len() >= 4
        && (segments[2] == "tree" || segments[2] == "blob")
    {
        let clean = format!("https://{}/{}/{}", host, segments[0], segments[1]);
        return (clean, Some(segments[3].to_string()));
    }

    if (host == "github.com" || host.ends_with(".github.com")) && segments.len() > 2 {
        return (
            format!("https://{}/{}/{}", host, segments[0], segments[1]),
            None,
        );
    }

    if (host == "gitlab.com" || host.ends_with(".gitlab.com"))
        && segments.len() >= 5
        && segments[2] == "-"
        && (segments[3] == "tree" || segments[3] == "blob")
    {
        let clean = format!("https://{}/{}/{}", host, segments[0], segments[1]);
        return (clean, Some(segments[4].to_string()));
    }

    if (host == "gitlab.com" || host.ends_with(".gitlab.com")) && segments.len() > 2 {
        return (
            format!("https://{}/{}/{}", host, segments[0], segments[1]),
            None,
        );
    }

    (raw.to_string(), None)
}

/// Returns a repository-relative folder selected by a GitHub/GitLab URL.
///
/// A repository URL has no subdirectory. Folder URLs may use either the
/// provider's normal `/tree/<branch>/...` form or a direct `/...` path.
pub fn source_subdirectory(raw: &str) -> Option<String> {
    let parsed = Url::parse(raw).ok()?;
    let host = parsed.host_str()?;
    let segments: Vec<&str> = parsed.path().trim_start_matches('/').split('/').collect();

    if host == "github.com" || host.ends_with(".github.com") {
        if segments.len() >= 5 && (segments[2] == "tree" || segments[2] == "blob") {
            return Some(segments[4..].join("/"));
        }
        if segments.len() > 2 && segments[2] != "tree" && segments[2] != "blob" {
            return Some(segments[2..].join("/"));
        }
    }

    if host == "gitlab.com" || host.ends_with(".gitlab.com") {
        if segments.len() >= 6
            && segments[2] == "-"
            && (segments[3] == "tree" || segments[3] == "blob")
        {
            return Some(segments[5..].join("/"));
        }
        if segments.len() > 2 && segments[2] != "-" {
            return Some(segments[2..].join("/"));
        }
    }

    None
}

pub fn clone_to_temp(
    raw_url: &str,
    resolved_ref: Option<&str>,
) -> Result<(TempDir, PathBuf, String)> {
    let (clean_url, branch) = clean_git_url(raw_url);
    let temp = TempDir::new()?;
    let checkout = temp.path().join("source");
    let mut clone = Command::new("git");
    clone.args(["clone", "--quiet"]);
    if resolved_ref.is_none() {
        clone.args(["--depth", "1"]);
        if let Some(branch) = branch.as_deref() {
            clone.args(["--branch", branch]);
        }
    }
    clone.arg(&clean_url).arg(&checkout);
    run_git(
        &mut clone,
        &format!("git clone failed for {clean_url}; is the repo accessible?"),
    )?;
    if let Some(reference) = resolved_ref {
        run_git(
            Command::new("git")
                .args(["checkout", "--quiet", "--detach", reference])
                .current_dir(&checkout),
            &format!("could not check out recorded ref {reference}"),
        )?;
    }
    Ok((temp, checkout, clean_url))
}

/// Clone a repository at one tag, shallowly. The caller reads the commit
/// the tag names with [`resolve_ref`]; the lockfile pins that, not the tag.
pub fn clone_tag_to_temp(raw_url: &str, tag: &str) -> Result<(TempDir, PathBuf, String)> {
    let (clean_url, _) = clean_git_url(raw_url);
    let temp = TempDir::new()?;
    let checkout = temp.path().join("source");
    run_git(
        Command::new("git")
            .args(["clone", "--quiet", "--depth", "1", "--branch", tag])
            .arg(&clean_url)
            .arg(&checkout),
        &format!("git clone failed for {clean_url} at tag {tag}; is the repo accessible?"),
    )?;
    Ok((temp, checkout, clean_url))
}

/// One tag a remote publishes and the commit it names right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTag {
    pub name: String,
    /// The commit, not the tag object: an annotated tag is peeled.
    pub commit: String,
}

/// The tags a repository publishes, without cloning it, each with the
/// commit it currently names. That commit is what a repointed tag changes.
pub fn list_remote_tags(raw_url: &str) -> Result<Vec<RemoteTag>> {
    let (clean_url, _) = clean_git_url(raw_url);
    let output = Command::new("git")
        .args(["ls-remote", "--tags", &clean_url])
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let context = format!("git ls-remote failed for {clean_url}; is the repo accessible?");
        return Err(TuffError::source_failed(if stderr.is_empty() {
            context
        } else {
            format!("{context}: {stderr}")
        }));
    }
    Ok(parse_ls_remote_tags(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

/// Read `git ls-remote --tags` output. A lightweight tag is one row naming
/// the commit. An annotated tag is two: the tag object, then a `^{}` row
/// naming the commit it points at, which is the one that matters.
fn parse_ls_remote_tags(output: &str) -> Vec<RemoteTag> {
    let mut tags: Vec<RemoteTag> = Vec::new();
    for line in output.lines() {
        let Some((sha, reference)) = line.split_once('\t') else {
            continue;
        };
        let Some(name) = reference.strip_prefix("refs/tags/") else {
            continue;
        };
        match name.strip_suffix("^{}") {
            Some(peeled) => {
                if let Some(tag) = tags.iter_mut().find(|tag| tag.name == peeled) {
                    tag.commit = sha.to_string();
                }
            }
            None => tags.push(RemoteTag {
                name: name.to_string(),
                commit: sha.to_string(),
            }),
        }
    }
    tags
}

fn run_git(cmd: &mut Command, context: &str) -> Result<()> {
    let output = cmd.output()?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(TuffError::source_failed(context.to_string()))
    } else {
        Err(TuffError::source_failed(format!("{context}: {stderr}")))
    }
}

pub fn resolve_ref(repo: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()?;

    if !output.status.success() {
        return Err(TuffError::source_failed("failed to resolve git ref"));
    }

    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sha.is_empty() {
        return Err(TuffError::source_failed("empty git ref"));
    }
    Ok(sha)
}

pub fn discover_capability(
    repo: &Path,
    name: &str,
    capability_type: CapabilityType,
) -> Result<PathBuf> {
    let dir_plural = capability_type.plural_dir(); // "skills", "tools", "hooks", "workflows"
    let dir_singular = capability_type.as_str(); // "skill", "tool", "hook", "workflow"

    let mut matches = Vec::new();

    // A URL-selected path can be nested arbitrarily deep, so try the exact
    // repository-relative path before the conventional capability layouts.
    let direct = repo.join(name);
    if direct.is_dir() {
        matches.push(direct);
    }

    // Pattern 1: <plural>/<name>/ (e.g. skills/security-review/)
    let p1 = repo.join(dir_plural).join(name);
    if p1.is_dir() {
        matches.push(p1);
    }

    // Pattern 2: <singular>/<name>/ (e.g. skill/security-review/)
    let p2 = repo.join(dir_singular).join(name);
    if p2.is_dir() {
        matches.push(p2);
    }

    // Pattern 3: <name>/ at root level
    let p3 = repo.join(name);
    if p3.is_dir() {
        matches.push(p3);
    }

    // Pattern 4: Walk <plural>/ subdirs for <category>/<name>/
    let plural_dir = repo.join(dir_plural);
    if plural_dir.is_dir() {
        for entry in std::fs::read_dir(&plural_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let candidate = entry.path().join(name);
                if candidate.is_dir() {
                    matches.push(candidate);
                }
            }
        }
    }

    matches.sort();
    matches.dedup();
    match matches.len() {
        0 => {
            let nearby = list_nearby_capabilities(repo, capability_type)?;
            let hint = if nearby.is_empty() {
                String::new()
            } else {
                format!("\nAvailable {dir_plural}: {}", nearby.join(", "))
            };
            Err(TuffError::not_found(format!(
                "{} '{}' not found in repository{hint}",
                capability_type, name
            )))
        }
        1 => Ok(matches[0].clone()),
        _ => {
            let paths: Vec<_> = matches
                .iter()
                .map(|p| p.strip_prefix(repo).unwrap_or(p).display().to_string())
                .collect();
            Err(TuffError::usage(format!(
                "ambiguous capability name '{}' matches multiple paths: {}",
                name,
                paths.join(", ")
            )))
        }
    }
}

fn list_nearby_capabilities(repo: &Path, capability_type: CapabilityType) -> Result<Vec<String>> {
    let dir_plural = capability_type.plural_dir();
    let capabilities_dir = repo.join(dir_plural);
    if !capabilities_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();
    for entry in std::fs::read_dir(&capabilities_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') {
                names.push(name);
            }
        }
    }
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ls_remote_tags_peel_annotated_tags_to_their_commit() {
        let output = "aaaa\trefs/tags/v1.0.0\n\
                      bbbb\trefs/tags/v1.2.0\n\
                      cccc\trefs/tags/v1.2.0^{}\n\
                      dddd\trefs/heads/main\n";
        let tags = parse_ls_remote_tags(output);
        assert_eq!(
            tags,
            vec![
                RemoteTag {
                    name: "v1.0.0".into(),
                    commit: "aaaa".into()
                },
                RemoteTag {
                    name: "v1.2.0".into(),
                    commit: "cccc".into()
                },
            ]
        );
    }

    #[test]
    fn detect_github_clean_url() {
        assert!(is_git_url("https://github.com/owner/repo"));
        assert!(is_git_url("http://github.com/owner/repo"));
    }

    #[test]
    fn detect_github_tree_url() {
        assert!(is_git_url("https://github.com/owner/repo/tree/main/skills"));
    }

    #[test]
    fn detect_ssh_url() {
        assert!(is_git_url("git@github.com:owner/repo.git"));
    }

    #[test]
    fn detect_file_url() {
        assert!(is_git_url("file:///path/to/repo"));
    }

    #[test]
    fn reject_local_path() {
        assert!(!is_git_url("./my-skill"));
        assert!(!is_git_url("/absolute/path"));
    }

    #[test]
    fn clean_github_tree_extracts_repo_and_branch() {
        let (url, branch) = clean_git_url("https://github.com/owner/repo/tree/main/skills");
        assert_eq!(url, "https://github.com/owner/repo");
        assert_eq!(branch, Some("main".to_string()));
    }

    #[test]
    fn clean_github_blob_extracts_repo_and_branch() {
        let (url, branch) = clean_git_url("https://github.com/owner/repo/blob/main/README.md");
        assert_eq!(url, "https://github.com/owner/repo");
        assert_eq!(branch, Some("main".to_string()));
    }

    #[test]
    fn clean_plain_url_passes_through() {
        let (url, branch) = clean_git_url("https://github.com/vercel-labs/skills");
        assert_eq!(url, "https://github.com/vercel-labs/skills");
        assert_eq!(branch, None);
    }

    #[test]
    fn github_folder_url_is_normalized_and_preserves_subdirectory() {
        let (url, branch) = clean_git_url(
            "https://github.com/am-will/codex-skills/hooks/aitmpl-codex/automation/change-logger",
        );
        assert_eq!(url, "https://github.com/am-will/codex-skills");
        assert_eq!(branch, None);
        assert_eq!(
            source_subdirectory(
                "https://github.com/am-will/codex-skills/hooks/aitmpl-codex/automation/change-logger"
            ),
            Some("hooks/aitmpl-codex/automation/change-logger".to_string())
        );
    }

    #[test]
    fn github_tree_url_preserves_branch_and_subdirectory() {
        assert_eq!(
            source_subdirectory(
                "https://github.com/am-will/codex-skills/tree/main/hooks/aitmpl-codex/automation/change-logger"
            ),
            Some("hooks/aitmpl-codex/automation/change-logger".to_string())
        );
    }

    #[test]
    fn clean_gitlab_tree_extracts_repo_and_branch() {
        let (url, branch) = clean_git_url("https://gitlab.com/owner/repo/-/tree/main/src");
        assert_eq!(url, "https://gitlab.com/owner/repo");
        assert_eq!(branch, Some("main".to_string()));
    }

    #[test]
    fn clean_file_url_passes_through() {
        let (url, branch) = clean_git_url("file:///path/to/repo");
        assert_eq!(url, "file:///path/to/repo");
        assert_eq!(branch, None);
    }
}
