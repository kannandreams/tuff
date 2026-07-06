use std::path::Path;

use crate::adapter::{PlannedFile, ResolvedCapability};
use crate::error::{CoralError, Result};
use crate::lockfile;

pub const ID: &str = "open-agents";
pub const DISPLAY_NAME: &str = "Open Agents";
pub const SUPPORTED_TYPES: &[&str] = &["skill", "tool", "hook", "workflow"];

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

pub fn supports(capability_type: &str) -> bool {
    SUPPORTED_TYPES.contains(&capability_type)
}

pub fn plan(capability: &ResolvedCapability, repo_root: &Path) -> Result<Vec<PlannedFile>> {
    match capability.capability_type.as_str() {
        "tool" => plan_tool(capability, repo_root),
        "hook" => plan_hook(capability, repo_root),
        "workflow" => plan_workflow(capability, repo_root),
        _ => plan_skill(capability, repo_root),
    }
}

fn plan_skill(capability: &ResolvedCapability, repo_root: &Path) -> Result<Vec<PlannedFile>> {
    if capability.source_files.is_empty() {
        return Err(CoralError::new("no source files to emit"));
    }

    let mut files = Vec::new();
    for (rel_path, content) in &capability.source_files {
        let target_path = repo_root
            .join(".agents")
            .join("skills")
            .join(&capability.id)
            .join(rel_path);

        files.push(PlannedFile {
            path: lockfile::relative_or_absolute_fs(&target_path, repo_root),
            content: content.clone(),
        });
    }
    Ok(files)
}

fn plan_tool(capability: &ResolvedCapability, repo_root: &Path) -> Result<Vec<PlannedFile>> {
    let mut files = Vec::new();

    // Copy source files
    for (rel_path, content) in &capability.source_files {
        let target_path = repo_root
            .join(".agents")
            .join("tools")
            .join(&capability.id)
            .join(rel_path);

        files.push(PlannedFile {
            path: lockfile::relative_or_absolute_fs(&target_path, repo_root),
            content: content.clone(),
        });
    }

    // If no source files, ensure tool dir exists (empty dir will just be created)
    if capability.source_files.is_empty() {
        let placeholder = repo_root
            .join(".agents")
            .join("tools")
            .join(&capability.id)
            .join(".gitkeep");
        files.push(PlannedFile {
            path: lockfile::relative_or_absolute_fs(&placeholder, repo_root),
            content: vec![],
        });
    }

    Ok(files)
}

fn plan_hook(capability: &ResolvedCapability, repo_root: &Path) -> Result<Vec<PlannedFile>> {
    let hook_cfg = capability.hook.as_ref().ok_or_else(|| {
        CoralError::new("hook capability requires [hook] section")
    })?;

    let target_path = repo_root
        .join(".agents")
        .join("hooks")
        .join(&capability.id)
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

fn plan_workflow(capability: &ResolvedCapability, repo_root: &Path) -> Result<Vec<PlannedFile>> {
    let wf = capability.workflow.as_ref().ok_or_else(|| {
        CoralError::new("workflow capability requires [[workflow.requires]] section")
    })?;

    let target_path = repo_root
        .join(".agents")
        .join("workflows")
        .join(&capability.id)
        .join("workflow.toml");

    let mut content = format!(
        "id = \"{}\"\nversion = \"{}\"\ntype = \"workflow\"\ndescription = \"{}\"\n",
        capability.id, capability.version, capability.description
    );
    for req in &wf.requires {
        content.push_str(&format!(
            "[[workflow.requires]]\nid = \"{}\"\ntype = \"{}\"\n",
            req.id, req.capability_type
        ));
    }

    Ok(vec![PlannedFile {
        path: lockfile::relative_or_absolute_fs(&target_path, repo_root),
        content: content.into_bytes(),
    }])
}

pub fn remove(primitive_id: &str, repo_root: &Path) -> Result<()> {
    remove_dir(repo_root, ".agents", "skills", primitive_id)?;
    remove_dir(repo_root, ".agents", "tools", primitive_id)?;
    remove_dir(repo_root, ".agents", "hooks", primitive_id)?;
    remove_dir(repo_root, ".agents", "workflows", primitive_id)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_returns_true_for_known_types() {
        assert!(supports("skill"));
        assert!(supports("tool"));
        assert!(supports("hook"));
    }

    #[test]
    fn supports_returns_false_for_unknown() {
        assert!(!supports("unknown"));
        assert!(!supports("policy"));
    }

    #[test]
    fn id_and_display_name_are_not_empty() {
        assert!(!ID.is_empty());
        assert!(!DISPLAY_NAME.is_empty());
    }
}
