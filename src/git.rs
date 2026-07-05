use std::{
    path::{Path, PathBuf},
    process::Command,
};

use sha2::{Digest, Sha256};
use url::Url;

use crate::error::{CoralError, Result};

pub fn is_git_url(s: &str) -> bool {
    s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("git@")
        || s.starts_with("file://")
}

fn hash_url(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn clean_git_url(raw: &str) -> (String, Option<String>) {
    let parsed = match Url::parse(raw) {
        Ok(u) => u,
        Err(_) => return (raw.to_string(), None),
    };

    let host = parsed.host_str().unwrap_or("");
    let path = parsed.path();
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();

    if host == "github.com" || host.ends_with(".github.com") {
        if segments.len() >= 4 && (segments[2] == "tree" || segments[2] == "blob") {
            let clean = format!("https://{}/{}/{}", host, segments[0], segments[1]);
            return (clean, Some(segments[3].to_string()));
        }
    }

    if host == "gitlab.com" || host.ends_with(".gitlab.com") {
        if segments.len() >= 5 && segments[2] == "-" && (segments[3] == "tree" || segments[3] == "blob")
        {
            let clean = format!("https://{}/{}/{}", host, segments[0], segments[1]);
            return (clean, Some(segments[4].to_string()));
        }
    }

    (raw.to_string(), None)
}

pub fn clone_or_fetch(raw_url: &str) -> Result<(PathBuf, String)> {
    let (clean_url, branch) = clean_git_url(raw_url);

    let home = dirs_home()?;
    let cache_dir = home
        .join(".coral")
        .join("cache")
        .join("git")
        .join(hash_url(&clean_url));

    if cache_dir.join(".git").exists() {
        let status = Command::new("git")
            .args(["fetch", "origin"])
            .current_dir(&cache_dir)
            .status()?;
        if !status.success() {
            return Err(CoralError::new(format!(
                "git fetch failed for {}",
                clean_url
            )));
        }

        let status = Command::new("git")
            .args(["reset", "--hard", "origin/HEAD"])
            .current_dir(&cache_dir)
            .status()?;
        if !status.success() {
            return Err(CoralError::new("git reset --hard failed"));
        }
    } else {
        std::fs::create_dir_all(cache_dir.parent().unwrap())?;

        let mut cmd = Command::new("git");
        cmd.args(["clone", "--depth", "1"]);
        if let Some(ref b) = branch {
            cmd.args(["--branch", b]);
        }
        cmd.arg(&clean_url).arg(&cache_dir);
        let status = cmd.status()?;
        if !status.success() {
            return Err(CoralError::new(format!(
                "git clone failed for {}; is the repo accessible?",
                clean_url
            )));
        }
    }

    Ok((cache_dir, clean_url))
}

pub fn resolve_ref(repo: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()?;

    if !output.status.success() {
        return Err(CoralError::new("failed to resolve git ref"));
    }

    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sha.is_empty() {
        return Err(CoralError::new("empty git ref"));
    }
    Ok(sha)
}

pub fn discover_skill(repo: &Path, name: &str) -> Result<PathBuf> {
    let mut matches = Vec::new();

    // Pattern 1: skills/<name>/SKILL.md
    let p1 = repo.join("skills").join(name);
    if p1.join("SKILL.md").exists() {
        matches.push(p1);
    }

    // Pattern 2: <name>/SKILL.md (root level)
    let p2 = repo.join(name);
    if p2.join("SKILL.md").exists() {
        matches.push(p2);
    }

    // Pattern 3: Walk skills/ subdirs for <category>/<name>/SKILL.md
    let skills_dir = repo.join("skills");
    if skills_dir.is_dir() {
        for entry in std::fs::read_dir(&skills_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let candidate = entry.path().join(name);
                if candidate.join("SKILL.md").exists() {
                    matches.push(candidate);
                }
            }
        }
    }

    match matches.len() {
        0 => {
            let nearby = list_nearby_skills(repo)?;
            let hint = if nearby.is_empty() {
                String::new()
            } else {
                format!("\nAvailable skills: {}", nearby.join(", "))
            };
            Err(CoralError::new(format!(
                "skill '{}' not found in repository{hint}",
                name
            )))
        }
        1 => Ok(matches[0].clone()),
        _ => {
            let paths: Vec<_> = matches
                .iter()
                .map(|p| p.strip_prefix(repo).unwrap_or(p).display().to_string())
                .collect();
            Err(CoralError::new(format!(
                "ambiguous skill name '{}' matches multiple paths: {}",
                name,
                paths.join(", ")
            )))
        }
    }
}

fn list_nearby_skills(repo: &Path) -> Result<Vec<String>> {
    let skills_dir = repo.join("skills");
    if !skills_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();
    for entry in std::fs::read_dir(&skills_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') && path.join("SKILL.md").exists() {
                skills.push(name);
            }
        }
    }
    skills.sort();
    Ok(skills)
}

fn dirs_home() -> Result<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .map_err(|_| CoralError::new("HOME environment variable not set"))
    }
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|_| {
                std::env::var("HOMEDRIVE")
                    .and_then(|hd| std::env::var("HOMEPATH").map(|hp| format!("{}{}", hd, hp)))
            })
            .map(PathBuf::from)
            .map_err(|_| CoralError::new("home directory not found"))
    }
}
