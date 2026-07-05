use std::path::Path;

use crate::adapter::{PlannedFile, ResolvedPrimitive};
use crate::error::{CoralError, Result};
use crate::lockfile;

pub const ID: &str = "open-agents";
pub const DISPLAY_NAME: &str = "Open Agents";
pub const SUPPORTED_PRIMITIVES: &[&str] = &["skill", "tool", "hook"];

pub const SUPPORTED_AGENTS: &[&str] = &[
    "Codex", "Cursor", "OpenCode", "GitHub Copilot",
    "Gemini CLI", "Roo", "Cline", "Windsurf",
];

pub const SUPPORTED_EVENTS: &[&str] = &[
    "before_finish",
    "after_save",
    "pre_tool_execution",
    "post_tool_execution",
];

pub fn supports(primitive: &str) -> bool {
    SUPPORTED_PRIMITIVES.contains(&primitive)
}

pub fn plan(primitive: &ResolvedPrimitive, repo_root: &Path) -> Result<Vec<PlannedFile>> {
    match primitive.primitive.as_str() {
        "tool" => plan_tool(primitive, repo_root),
        "hook" => plan_hook(primitive, repo_root),
        _ => plan_skill(primitive, repo_root),
    }
}

fn plan_skill(primitive: &ResolvedPrimitive, repo_root: &Path) -> Result<Vec<PlannedFile>> {
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

fn plan_tool(primitive: &ResolvedPrimitive, repo_root: &Path) -> Result<Vec<PlannedFile>> {
    let mut files = Vec::new();

    // Copy source files
    for (rel_path, content) in &primitive.source_files {
        let target_path = repo_root
            .join(".agents")
            .join("tools")
            .join(&primitive.id)
            .join(rel_path);

        files.push(PlannedFile {
            path: lockfile::relative_or_absolute_fs(&target_path, repo_root),
            content: content.clone(),
        });
    }

    // If no source files, ensure tool dir exists (empty dir will just be created)
    if primitive.source_files.is_empty() {
        let placeholder = repo_root
            .join(".agents")
            .join("tools")
            .join(&primitive.id)
            .join(".gitkeep");
        files.push(PlannedFile {
            path: lockfile::relative_or_absolute_fs(&placeholder, repo_root),
            content: vec![],
        });
    }

    Ok(files)
}

fn plan_hook(primitive: &ResolvedPrimitive, repo_root: &Path) -> Result<Vec<PlannedFile>> {
    let hook_cfg = primitive.hook.as_ref().ok_or_else(|| {
        CoralError::new("hook primitive requires [hook] section")
    })?;

    let target_path = repo_root
        .join(".agents")
        .join("hooks")
        .join(&primitive.id)
        .join("hook.toml");

    let content = format!(
        "event = \"{}\"\ncommand = \"{}\"\nworking_directory = \"{}\"\n",
        hook_cfg.event, hook_cfg.command, hook_cfg.working_directory
    );

    Ok(vec![PlannedFile {
        path: lockfile::relative_or_absolute_fs(&target_path, repo_root),
        content: content.into_bytes(),
    }])
}

pub fn remove(primitive_id: &str, repo_root: &Path) -> Result<()> {
    remove_dir(repo_root, ".agents", "skills", primitive_id)?;
    remove_dir(repo_root, ".agents", "tools", primitive_id)?;
    remove_dir(repo_root, ".agents", "hooks", primitive_id)?;

    // Clean MCP config
    let mcp_path = repo_root.join(".agents").join("mcp.json");
    super::mcp_remove_tool(repo_root, &mcp_path, primitive_id)?;

    Ok(())
}

fn remove_dir(repo_root: &Path, base: &str, kind: &str, primitive_id: &str) -> Result<()> {
    let dir = repo_root.join(base).join(kind).join(primitive_id);

    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }

    let kind_dir = dir.parent().expect("kind dir should have parent");
    if kind_dir.exists() {
        let mut rd = match std::fs::read_dir(kind_dir) {
            Ok(rd) => rd,
            Err(_) => return Ok(()),
        };
        if rd.next().is_none() {
            std::fs::remove_dir(kind_dir)?;
        }
    }

    let base_dir = kind_dir.parent().expect("base dir should have parent");
    if base_dir.exists() {
        let mut rd = match std::fs::read_dir(base_dir) {
            Ok(rd) => rd,
            Err(_) => return Ok(()),
        };
        if rd.next().is_none() {
            std::fs::remove_dir(base_dir)?;
        }
    }

    Ok(())
}

#[allow(dead_code)]
pub fn detect(repo_root: &Path) -> bool {
    repo_root.join(".agents").join("skills").exists()
}
