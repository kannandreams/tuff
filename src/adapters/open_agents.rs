use std::path::Path;

use crate::adapter::{PlannedFile, ResolvedPrimitive};
use crate::error::{CoralError, Result};
use crate::lockfile;

pub const ID: &str = "open-agents";
pub const DISPLAY_NAME: &str = "Open Agents";
pub const SUPPORTED_KINDS: &[&str] = &["skill"];

pub const SUPPORTED_AGENTS: &[&str] = &[
    "Codex", "Cursor", "OpenCode", "GitHub Copilot",
    "Gemini CLI", "Roo", "Cline", "Windsurf",
];

pub fn supports(primitive: &str) -> bool {
    SUPPORTED_KINDS.contains(&primitive)
}

pub fn plan(primitive: &ResolvedPrimitive, repo_root: &Path) -> Result<Vec<PlannedFile>> {
    if primitive.source_files.is_empty() {
        return Err(CoralError::new("no source files to emit"));
    }

    let mut files = Vec::new();
    for (rel_path, content) in &primitive.source_files {
        let target_path = repo_root
            .join(".agents")
            .join("skills")
            .join(&primitive.id)
            .join(rel_path);

        files.push(PlannedFile {
            path: lockfile::relative_or_absolute_fs(&target_path, repo_root),
            content: content.clone(),
        });
    }
    Ok(files)
}

pub fn remove(primitive_id: &str, repo_root: &Path) -> Result<()> {
    let skill_dir = repo_root
        .join(".agents")
        .join("skills")
        .join(primitive_id);

    if skill_dir.exists() {
        std::fs::remove_dir_all(&skill_dir)?;
    }

    let skills_dir = skill_dir
        .parent()
        .expect("skills dir should have parent");
    if skills_dir.exists() {
        let mut rd = match std::fs::read_dir(skills_dir) {
            Ok(rd) => rd,
            Err(_) => return Ok(()),
        };
        if rd.next().is_none() {
            std::fs::remove_dir(skills_dir)?;
        }
    }

    let agents_dir = skills_dir
        .parent()
        .expect("agents dir should have parent");
    if agents_dir.exists() {
        let mut rd = match std::fs::read_dir(agents_dir) {
            Ok(rd) => rd,
            Err(_) => return Ok(()),
        };
        if rd.next().is_none() {
            std::fs::remove_dir(agents_dir)?;
        }
    }

    Ok(())
}

#[allow(dead_code)]
pub fn detect(repo_root: &Path) -> bool {
    repo_root.join(".agents").join("skills").exists()
}
