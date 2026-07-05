use std::path::Path;

use crate::adapter::{PlannedFile, ResolvedPrimitive};
use crate::error::{CoralError, Result};
use crate::lockfile;

pub const ID: &str = "claude-code";
pub const DISPLAY_NAME: &str = "Claude Code";
pub const SUPPORTED_KINDS: &[&str] = &["skill"];

pub fn supports(kind: &str) -> bool {
    SUPPORTED_KINDS.contains(&kind)
}

pub fn plan(primitive: &ResolvedPrimitive, repo_root: &Path) -> Result<Vec<PlannedFile>> {
    let content = primitive
        .source_files
        .first()
        .ok_or_else(|| CoralError::new("no source files to emit"))?
        .clone();

    let target_path = repo_root
        .join(".claude")
        .join("skills")
        .join(&primitive.id)
        .join("SKILL.md");

    Ok(vec![PlannedFile {
        path: lockfile::relative_or_absolute_fs(&target_path, repo_root),
        content,
    }])
}

pub fn remove(primitive_id: &str, repo_root: &Path) -> Result<()> {
    let skill_dir = repo_root
        .join(".claude")
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

    let claude_dir = skills_dir
        .parent()
        .expect("claude dir should have parent");
    if claude_dir.exists() {
        let mut rd = match std::fs::read_dir(claude_dir) {
            Ok(rd) => rd,
            Err(_) => return Ok(()),
        };
        if rd.next().is_none() {
            std::fs::remove_dir(claude_dir)?;
        }
    }

    Ok(())
}

#[allow(dead_code)]
pub fn detect(repo_root: &Path) -> bool {
    repo_root.join(".claude").exists() || repo_root.join("CLAUDE.md").exists()
}
