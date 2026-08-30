//! RFC-103 tier 1: a generated, per-harness "capability index" skill.
//!
//! Tuff installs tools and workflows as real files, but nothing tells the
//! *agent* they exist — no runtime surface mentions them. This module
//! regenerates `<dir_prefix>/skills/tuff-capabilities/SKILL.md` for every
//! configured harness, listing installed tools and workflows with their
//! exact invocation command, so the one runtime surface every harness
//! already reads (skills) carries the hint. It is called at the end of
//! every install/update/delete path so the index can never go stale.
//!
//! Skills are excluded from the index (already natively visible, no
//! indirection needed) and hooks are excluded (they fire automatically,
//! there is nothing to "invoke"). External MCP servers (RFC-102) are listed
//! for discoverability — the harness already loads them, but the agent has
//! no other way to learn what a server is *for*.

use std::collections::BTreeMap;
use std::path::Path;

use crate::adapter::{self, AgentAdapter};
use crate::error::Result;
use crate::lockfile::{self, CapabilityLockEntry, TargetLockEntry};
use crate::manifest::CapabilityType;

use super::add::write_planned_file;
use super::hooks::registered_adapters;

pub(crate) const CAPABILITY_INDEX_ID: &str = "tuff-capabilities";

const INDEX_DESCRIPTION: &str = "Index of project tools and workflows installed via Tuff. Consult when a task involves one of the capabilities listed below, to find the exact invocation command.";

/// Regenerate the capability-index skill for every harness configured in
/// this project. Safe to call after any install/update/delete — it derives
/// the index entirely from the lockfile, so calling it when nothing
/// relevant changed is a cheap no-op (the render is compared against the
/// last-known content hash before anything is written).
pub(crate) fn regenerate_capability_index(repo_root: &Path) -> Result<()> {
    let mut lockfile = lockfile::require_lockfile(repo_root)?;
    let adapters = registered_adapters(repo_root)?;

    let mut new_targets: BTreeMap<String, TargetLockEntry> = BTreeMap::new();

    for adapter in &adapters {
        let tools = indexable(&lockfile, CapabilityType::Tool, adapter.id());
        let workflows = indexable(&lockfile, CapabilityType::Workflow, adapter.id());
        let servers = indexable(&lockfile, CapabilityType::McpServer, adapter.id());

        if tools.is_empty() && workflows.is_empty() && servers.is_empty() {
            let already_indexed = lockfile
                .capabilities
                .get(CAPABILITY_INDEX_ID)
                .is_some_and(|entry| entry.targets.contains_key(adapter.id()));
            if already_indexed {
                adapter.remove(CAPABILITY_INDEX_ID, repo_root, &[])?;
            }
            continue;
        }

        let content = render_skill(adapter.dir_prefix(), &tools, &workflows, &servers);
        let content_hash = lockfile::hash_bytes(&content);

        let skill_path = repo_root
            .join(adapter.dir_prefix())
            .join("skills")
            .join(CAPABILITY_INDEX_ID)
            .join("SKILL.md");
        let existing_target = lockfile
            .capabilities
            .get(CAPABILITY_INDEX_ID)
            .and_then(|entry| entry.targets.get(adapter.id()));
        let unchanged = skill_path.exists()
            && existing_target
                .and_then(|target| target.emitted_files.first())
                .is_some_and(|file| file.hash == content_hash);
        if unchanged {
            new_targets.insert(adapter.id().to_string(), existing_target.unwrap().clone());
            continue;
        }

        let resolved = adapter::ResolvedCapability {
            id: CAPABILITY_INDEX_ID.to_string(),
            capability_type: CapabilityType::Skill,
            version: "1.0.0".to_string(),
            description: INDEX_DESCRIPTION.to_string(),
            source_files: vec![("SKILL.md".to_string(), content)],
            source_dir: repo_root.to_path_buf(),
            kind: adapter::CapabilityKind::Skill,
        };
        let planned_files = adapter.plan(&resolved, repo_root)?;

        let mut emitted = Vec::new();
        for planned in &planned_files {
            emitted.push(write_planned_file(repo_root, planned)?);
        }

        let installed_root = repo_root
            .join(adapter.dir_prefix())
            .join("skills")
            .join(CAPABILITY_INDEX_ID);
        let baseline_hash = crate::cache::hash_tree(&installed_root)?;
        crate::cache::populate(&super::home_dir()?, &baseline_hash, &installed_root)?;

        new_targets.insert(
            adapter.id().to_string(),
            TargetLockEntry {
                emitted_files: emitted,
                managed_hooks: Vec::new(),
                ownership: lockfile::TargetOwnership::Generated,
                sha256: baseline_hash,
                installed_path: lockfile::relative_or_absolute_fs(&installed_root, repo_root),
            },
        );
    }

    if new_targets.is_empty() {
        if lockfile.capabilities.remove(CAPABILITY_INDEX_ID).is_some() {
            lockfile::write_lockfile(repo_root, &lockfile)?;
        }
        return Ok(());
    }

    lockfile.capabilities.insert(
        CAPABILITY_INDEX_ID.to_string(),
        CapabilityLockEntry {
            capability_type: CapabilityType::Skill,
            installed_version: "1.0.0".to_string(),
            description: INDEX_DESCRIPTION.to_string(),
            source_path: "<generated>".to_string(),
            targets: new_targets,
            source: None,
            scope: "project".to_string(),
            pack: None,
            implementation: None,
            parameters: None,
            workflow: None,
            server: None,
        },
    );
    lockfile::write_lockfile(repo_root, &lockfile)
}

fn indexable<'a>(
    lockfile: &'a lockfile::Lockfile,
    kind: CapabilityType,
    target_id: &str,
) -> Vec<(&'a String, &'a CapabilityLockEntry)> {
    let mut rows: Vec<(&String, &CapabilityLockEntry)> = lockfile
        .capabilities
        .iter()
        .filter(|(id, entry)| {
            id.as_str() != CAPABILITY_INDEX_ID
                && entry.capability_type == kind
                && entry.targets.contains_key(target_id)
        })
        .collect();
    rows.sort_by_key(|(id, _)| id.as_str());
    rows
}

fn render_skill(
    dir_prefix: &str,
    tools: &[(&String, &CapabilityLockEntry)],
    workflows: &[(&String, &CapabilityLockEntry)],
    servers: &[(&String, &CapabilityLockEntry)],
) -> Vec<u8> {
    let mut body = String::new();
    body.push_str("# Installed capabilities\n");

    if !tools.is_empty() {
        body.push_str("\n## Tools\n");
        for (id, entry) in tools {
            render_tool(&mut body, dir_prefix, id, entry);
        }
    }

    if !workflows.is_empty() {
        body.push_str("\n## Workflows\n");
        for (id, entry) in workflows {
            render_workflow(&mut body, id, entry);
        }
    }

    if !servers.is_empty() {
        body.push_str("\n## MCP Servers\n");
        body.push_str("These are already loaded by the harness; call their tools directly.\n");
        for (id, entry) in servers {
            render_mcp_server(&mut body, id, entry);
        }
    }

    let frontmatter = format!(
        "---\nname: {CAPABILITY_INDEX_ID}\ndescription: \"{}\"\n---\n\n",
        yaml_quote(INDEX_DESCRIPTION)
    );
    (frontmatter + &body).into_bytes()
}

fn render_tool(body: &mut String, dir_prefix: &str, id: &str, entry: &CapabilityLockEntry) {
    body.push_str(&format!(
        "\n### {} — {}\n",
        sanitize_inline(id),
        sanitize_inline(&entry.description)
    ));

    match &entry.implementation {
        Some(implementation) => {
            let entrypoint_path = format!("{dir_prefix}/tools/{id}/{}", implementation.entrypoint);
            body.push_str(&format!(
                "Run: `{} {entrypoint_path} '<json-args>'`\n",
                implementation.language
            ));
        }
        None => {
            body.push_str(
                "Run: invocation details unavailable — reinstall this tool to populate them.\n",
            );
        }
    }

    if let Some(args) = render_arguments(entry.parameters.as_ref()) {
        body.push_str(&args);
    }
}

fn render_arguments(parameters: Option<&serde_json::Value>) -> Option<String> {
    let properties = parameters?.get("properties")?.as_object()?;
    if properties.is_empty() {
        return None;
    }
    let required: Vec<&str> = parameters
        .and_then(|p| p.get("required"))
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect()
        })
        .unwrap_or_default();

    let mut out = String::from("Arguments (JSON object on argv[1]):\n");
    for (name, schema) in properties {
        let ty = schema
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("any");
        let requiredness = if required.contains(&name.as_str()) {
            "required"
        } else {
            "optional"
        };
        let description = schema
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        out.push_str(&format!(
            "- `{}` ({ty}, {requiredness}){}\n",
            sanitize_inline(name),
            if description.is_empty() {
                String::new()
            } else {
                format!(": {}", sanitize_inline(description))
            }
        ));
    }
    Some(out)
}

fn render_workflow(body: &mut String, id: &str, entry: &CapabilityLockEntry) {
    body.push_str(&format!(
        "\n### {} — {}\n",
        sanitize_inline(id),
        sanitize_inline(&entry.description)
    ));

    match &entry.workflow {
        Some(workflow) if !workflow.requires.is_empty() => {
            body.push_str("Steps:\n");
            for (index, requirement) in workflow.requires.iter().enumerate() {
                body.push_str(&format!(
                    "{}. {} ({})\n",
                    index + 1,
                    sanitize_inline(&requirement.id),
                    requirement.capability_type.as_str()
                ));
            }
        }
        _ => {
            body.push_str("Steps: unavailable — reinstall this workflow to populate them.\n");
        }
    }
}

fn render_mcp_server(body: &mut String, id: &str, entry: &CapabilityLockEntry) {
    body.push_str(&format!(
        "\n### {} — {}\n",
        sanitize_inline(id),
        sanitize_inline(&entry.description)
    ));
    let Some(server) = &entry.server else {
        body.push_str("Details unavailable — reinstall this server to populate them.\n");
        return;
    };
    body.push_str(&format!("Transport: {}\n", server.transport.as_str()));
    if let Some(summary) = server
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.tools_summary.as_deref())
        .filter(|summary| !summary.trim().is_empty())
    {
        body.push_str(&format!("Tools: {}\n", sanitize_inline(summary)));
    }
}

/// Strip characters from user-controlled manifest text (descriptions,
/// argument descriptions) that could inject a fake heading or otherwise
/// distort the generated document's structure — the same class of risk RFC-
/// 103 flags for this generator, and the same shape as the TOML-escaping
/// bug in debt #4. This text never reaches the YAML frontmatter (which only
/// ever holds the fixed `INDEX_DESCRIPTION`), so the concern here is
/// markdown-body structure, not frontmatter parsing.
fn sanitize_inline(text: &str) -> String {
    let collapsed: String = text
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    let trimmed = collapsed.trim();
    const MAX_LEN: usize = 300;
    if trimmed.chars().count() > MAX_LEN {
        let truncated: String = trimmed.chars().take(MAX_LEN).collect();
        format!("{truncated}…")
    } else {
        trimmed.to_string()
    }
}

fn yaml_quote(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lockfile::TargetOwnership;
    use crate::manifest::{ImplementationConfig, Requirement, WorkflowConfig};
    use std::collections::BTreeMap as Map;

    fn tool_entry(
        description: &str,
        language: &str,
        entrypoint: &str,
        parameters: serde_json::Value,
        target: &str,
    ) -> CapabilityLockEntry {
        let mut targets = Map::new();
        targets.insert(
            target.to_string(),
            TargetLockEntry {
                emitted_files: Vec::new(),
                managed_hooks: Vec::new(),
                ownership: TargetOwnership::Generated,
                sha256: String::new(),
                installed_path: String::new(),
            },
        );
        CapabilityLockEntry {
            capability_type: CapabilityType::Tool,
            installed_version: "1.0.0".into(),
            description: description.into(),
            source_path: String::new(),
            targets,
            source: None,
            scope: "project".into(),
            pack: None,
            implementation: Some(ImplementationConfig {
                language: language.into(),
                entrypoint: entrypoint.into(),
                mcp: false,
                runtime_deps: Vec::new(),
            }),
            parameters: Some(parameters),
            workflow: None,
            server: None,
        }
    }

    #[test]
    fn renders_run_command_and_arguments_for_claude_prefix() {
        let entry = tool_entry(
            "Scan a directory for common security vulnerabilities.",
            "node",
            "index.js",
            serde_json::json!({
                "type": "object",
                "required": ["target_dir"],
                "properties": {
                    "target_dir": {"type": "string", "description": "Directory to scan"},
                    "severity": {"type": "string"}
                }
            }),
            "claude",
        );
        let id = "security-review".to_string();
        let mut body = String::new();
        render_tool(&mut body, ".claude", &id, &entry);

        assert!(body.contains("### security-review — Scan a directory"));
        assert!(body.contains("Run: `node .claude/tools/security-review/index.js '<json-args>'`"));
        assert!(body.contains("- `target_dir` (string, required): Directory to scan"));
        assert!(body.contains("- `severity` (string, optional)"));
    }

    #[test]
    fn renders_shared_agents_prefix_for_codex_and_open_agents() {
        let entry = tool_entry(
            "Runs a thing.",
            "bash",
            "run.sh",
            serde_json::json!({"type": "object", "properties": {}}),
            "codex",
        );
        let id = "runner".to_string();
        let mut body = String::new();
        render_tool(&mut body, ".agents", &id, &entry);
        assert!(body.contains("Run: `bash .agents/tools/runner/run.sh '<json-args>'`"));
    }

    #[test]
    fn missing_implementation_degrades_gracefully() {
        let mut entry = tool_entry("desc", "node", "index.js", serde_json::json!({}), "claude");
        entry.implementation = None;
        entry.parameters = None;
        let id = "legacy-tool".to_string();
        let mut body = String::new();
        render_tool(&mut body, ".claude", &id, &entry);
        assert!(body.contains("invocation details unavailable"));
    }

    #[test]
    fn workflow_steps_render_from_requires() {
        let mut entry = tool_entry(
            "Pre-release checks.",
            "node",
            "x",
            serde_json::json!({}),
            "claude",
        );
        entry.capability_type = CapabilityType::Workflow;
        entry.implementation = None;
        entry.parameters = None;
        entry.workflow = Some(WorkflowConfig {
            requires: vec![
                Requirement {
                    id: "security-review".into(),
                    capability_type: CapabilityType::Tool,
                },
                Requirement {
                    id: "pre-commit-lint".into(),
                    capability_type: CapabilityType::Hook,
                },
            ],
        });
        let id = "release-prep".to_string();
        let mut body = String::new();
        render_workflow(&mut body, &id, &entry);
        assert!(body.contains("1. security-review (tool)"));
        assert!(body.contains("2. pre-commit-lint (hook)"));
    }

    #[test]
    fn sanitize_inline_strips_newlines_and_caps_length() {
        let malicious = "legit text\n### fake-heading\nmore text";
        let cleaned = sanitize_inline(malicious);
        assert!(!cleaned.contains('\n'));
        assert!(cleaned.contains("fake-heading"));

        let long = "a".repeat(500);
        let cleaned_long = sanitize_inline(&long);
        assert!(cleaned_long.chars().count() <= 301);
    }

    #[test]
    fn frontmatter_is_fixed_and_unaffected_by_manifest_content() {
        let entry = tool_entry(
            "desc with \n newline and --- dashes",
            "node",
            "index.js",
            serde_json::json!({}),
            "claude",
        );
        let id = "x".to_string();
        let tools = vec![(&id, &entry)];
        let bytes = render_skill(".claude", &tools, &[], &[]);
        let content = String::from_utf8(bytes).unwrap();
        let mut parts = content.splitn(3, "---\n");
        assert_eq!(parts.next(), Some(""));
        let frontmatter = parts.next().unwrap();
        assert!(frontmatter.contains(&format!("name: {CAPABILITY_INDEX_ID}")));
        assert!(frontmatter.contains("description:"));
        // The manifest's own newline/`---` never reach the frontmatter block.
        assert!(!frontmatter.contains("dashes"));
    }
}
