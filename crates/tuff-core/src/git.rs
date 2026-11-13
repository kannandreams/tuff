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
    let listing = ls_remote(&clean_url, &["--tags"], &[])?;
    Ok(parse_ls_remote_tags(&listing))
}

/// What one `ls-remote` says about a repository: the commit its HEAD names
/// and every tag it publishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRefs {
    /// The commit HEAD names now: the branch a `/tree/<branch>` URL selects,
    /// otherwise the remote's default branch. `None` when the remote
    /// advertises no such ref, as for an empty repository or a `/tree/`
    /// segment that names a commit rather than a branch.
    pub head: Option<String>,
    pub tags: Vec<RemoteTag>,
}

/// HEAD and the tags in one round trip, without cloning. `outdated` uses
/// this so that "did HEAD move" and "does the repository publish releases"
/// cost one network call between them rather than a clone plus a listing.
pub fn list_remote_refs(raw_url: &str) -> Result<RemoteRefs> {
    let (clean_url, branch) = clean_git_url(raw_url);
    let head_ref = match branch {
        Some(branch) => format!("refs/heads/{branch}"),
        None => "HEAD".to_string(),
    };
    let listing = ls_remote(&clean_url, &[], &[&head_ref, "refs/tags/*"])?;
    let head = listing.lines().find_map(|line| {
        let (sha, reference) = line.split_once('\t')?;
        (reference == head_ref).then(|| sha.to_string())
    });
    Ok(RemoteRefs {
        head,
        tags: parse_ls_remote_tags(&listing),
    })
}

/// Run `git ls-remote <flags> <url> <patterns>` and return its stdout.
fn ls_remote(clean_url: &str, flags: &[&str], patterns: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("ls-remote")
        .args(flags)
        .arg(clean_url)
        .args(patterns)
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
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
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
    locate_capability(repo, name, capability_type).map(|found| found.dir)
}

/// Where [`locate_capability`] found a capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedCapability {
    pub dir: PathBuf,
    /// True when no conventional layout matched and the directory was found
    /// by searching the repository. The caller then records the
    /// repository-relative path, so later updates go straight to it.
    pub searched: bool,
}

/// A capability found by searching a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundCapability {
    /// The capability's name: the `name:` in its `SKILL.md` frontmatter,
    /// else its directory name.
    pub name: String,
    /// The directory, relative to the repository root, with `/` separators.
    pub path: String,
}

/// Directories never searched for capabilities: version-control metadata
/// and dependency trees, which can be large and hold other projects' skills.
const SKIPPED_SEARCH_DIRS: &[&str] = &[".git", "node_modules", "target", ".venv", "vendor"];

/// How deep [`find_capabilities`] looks below the repository root.
const MAX_SEARCH_DEPTH: usize = 8;

/// Find a capability named `name` in a checked-out repository.
///
/// The conventional layouts are tried first, so a repository that follows
/// them resolves exactly as before. When none matches, the whole repository
/// is searched for capability directories whose directory name or
/// `SKILL.md` `name:` equals `name`, which covers layouts such as
/// `.claude/skills/<name>` or `plugins/<plugin>/skills/<name>`.
pub fn locate_capability(
    repo: &Path,
    name: &str,
    capability_type: CapabilityType,
) -> Result<LocatedCapability> {
    let dir_plural = capability_type.plural_dir(); // "skills", "tools", "hooks", "mcp-servers"
    let dir_singular = capability_type.as_str(); // "skill", "tool", "hook", "mcp-server"

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
        0 => {}
        1 => {
            return Ok(LocatedCapability {
                dir: matches.remove(0),
                searched: false,
            });
        }
        _ => return Err(ambiguous(repo, name, &matches)),
    }

    let found = find_capabilities(repo, capability_type)?;
    let searched: Vec<PathBuf> = found
        .iter()
        .filter(|capability| {
            capability.name == name || capability.path.rsplit('/').next() == Some(name)
        })
        .map(|capability| repo.join(&capability.path))
        .collect();
    match searched.len() {
        0 => Err(not_found(name, capability_type, &found)),
        1 => Ok(LocatedCapability {
            dir: searched.into_iter().next().expect("one match"),
            searched: true,
        }),
        _ => Err(ambiguous(repo, name, &searched)),
    }
}

/// Every capability of `capability_type` in a checked-out repository,
/// sorted by path. A skill is a directory holding `SKILL.md`; any other
/// type is a directory holding a `tuff.toml`.
pub fn find_capabilities(
    repo: &Path,
    capability_type: CapabilityType,
) -> Result<Vec<FoundCapability>> {
    let marker = if capability_type == CapabilityType::Skill {
        "SKILL.md"
    } else {
        "tuff.toml"
    };
    let mut found = Vec::new();
    let mut pending = vec![(repo.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = pending.pop() {
        if dir != repo && dir.join(marker).is_file() {
            let path = dir
                .strip_prefix(repo)
                .unwrap_or(&dir)
                .components()
                .map(|component| component.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("/");
            let dir_name = dir
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            let name = if capability_type == CapabilityType::Skill {
                std::fs::read_to_string(dir.join(marker))
                    .ok()
                    .and_then(|skill| crate::manifest::frontmatter_name(&skill))
                    .unwrap_or(dir_name)
            } else {
                dir_name
            };
            found.push(FoundCapability { name, path });
            // A capability's own subdirectories are its files, not more
            // capabilities.
            continue;
        }
        if depth >= MAX_SEARCH_DEPTH {
            continue;
        }
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            // `file_type` does not follow symlinks, so a link cycle cannot
            // keep the search going.
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let file_name = entry.file_name();
            if SKIPPED_SEARCH_DIRS
                .iter()
                .any(|skipped| file_name == std::ffi::OsStr::new(skipped))
            {
                continue;
            }
            pending.push((entry.path(), depth + 1));
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

fn ambiguous(repo: &Path, name: &str, matches: &[PathBuf]) -> TuffError {
    let paths: Vec<_> = matches
        .iter()
        .map(|p| p.strip_prefix(repo).unwrap_or(p).display().to_string())
        .collect();
    TuffError::usage(format!(
        "ambiguous capability name '{}' matches multiple paths: {}",
        name,
        paths.join(", ")
    ))
    .with_hint("name the folder in the URL instead, e.g. <repo>/tree/<branch>/<path>")
}

fn not_found(name: &str, capability_type: CapabilityType, found: &[FoundCapability]) -> TuffError {
    let mut message = format!("{capability_type} '{name}' not found in repository");
    if !found.is_empty() {
        message.push_str(&format!("\n{}", describe_found(capability_type, found)));
    }
    TuffError::not_found(message).with_hint(
        "pass one of the names above, or name the folder in the URL, e.g. <repo>/tree/<branch>/<path>",
    )
}

/// The "Available skills:" block shared by every error that lists what a
/// repository offers: one `name  path` line per capability.
pub fn describe_found(capability_type: CapabilityType, found: &[FoundCapability]) -> String {
    let width = found.iter().map(|c| c.name.len()).max().unwrap_or(0);
    let mut lines = vec![format!("Available {}:", capability_type.plural_dir())];
    for capability in found {
        lines.push(format!(
            "  {:width$}  {}",
            capability.name,
            capability.path,
            width = width
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill(repo: &Path, path: &str, name: &str) {
        let dir = repo.join(path);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: d\n---\nbody\n"),
        )
        .unwrap();
    }

    #[test]
    fn find_capabilities_searches_the_repository_and_skips_dependency_trees() {
        let repo = tempfile::tempdir().unwrap();
        write_skill(repo.path(), "skills/alpha", "alpha");
        write_skill(repo.path(), ".claude/skills/beta", "beta");
        write_skill(repo.path(), "skills/zeta-skill", "zeta");
        write_skill(repo.path(), "node_modules/dep/skills/beta", "beta");
        // A skill's own subfolders are its files, not more skills.
        write_skill(repo.path(), "skills/alpha/examples/inner", "inner");

        let found = find_capabilities(repo.path(), CapabilityType::Skill).unwrap();
        let pairs: Vec<_> = found
            .iter()
            .map(|c| (c.name.as_str(), c.path.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("beta", ".claude/skills/beta"),
                ("alpha", "skills/alpha"),
                ("zeta", "skills/zeta-skill"),
            ]
        );
    }

    #[test]
    fn locate_capability_prefers_conventional_layouts_then_searches() {
        let repo = tempfile::tempdir().unwrap();
        write_skill(repo.path(), "skills/alpha", "alpha");
        write_skill(repo.path(), "plugins/p/skills/gamma", "gamma");
        write_skill(repo.path(), "skills/zeta-skill", "zeta");

        let alpha = locate_capability(repo.path(), "alpha", CapabilityType::Skill).unwrap();
        assert!(!alpha.searched);
        assert_eq!(alpha.dir, repo.path().join("skills/alpha"));

        let gamma = locate_capability(repo.path(), "gamma", CapabilityType::Skill).unwrap();
        assert!(gamma.searched);
        assert_eq!(gamma.dir, repo.path().join("plugins/p/skills/gamma"));

        // Matched by the frontmatter name, not the folder.
        let zeta = locate_capability(repo.path(), "zeta", CapabilityType::Skill).unwrap();
        assert_eq!(zeta.dir, repo.path().join("skills/zeta-skill"));

        let missing = locate_capability(repo.path(), "nope", CapabilityType::Skill)
            .unwrap_err()
            .to_string();
        assert!(missing.contains("plugins/p/skills/gamma"), "{missing}");
    }

    #[test]
    fn locate_capability_reports_a_name_found_twice_by_search() {
        let repo = tempfile::tempdir().unwrap();
        write_skill(repo.path(), "a/skills/dup", "dup");
        write_skill(repo.path(), "b/skills/dup", "dup");
        let err = locate_capability(repo.path(), "dup", CapabilityType::Skill)
            .unwrap_err()
            .to_string();
        assert!(err.contains("ambiguous"), "{err}");
        assert!(
            err.contains("a/skills/dup") && err.contains("b/skills/dup"),
            "{err}"
        );
    }

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
