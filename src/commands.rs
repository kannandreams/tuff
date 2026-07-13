use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const CORAL_GUIDE_CONTENT: &str = include_str!("../assets/coral-cli-guide.md");

use tabled::{
    settings::{object::Columns, Modify, Style, Width},
    Table, Tabled,
};

use crate::{
    adapter::{self, AdapterKind, resolve_capability},
    config, git,
    display,
    error::{CoralError, Result},
    lockfile::{self, TargetLockEntry},
    manifest::{self, load_manifest},
    resolver::{self, Scope},
};

#[derive(Tabled)]
struct ListRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "TYPE")]
    capability_type: String,
    #[tabled(rename = "VERSION")]
    version: String,
    #[tabled(rename = "SCOPE")]
    scope: String,
    #[tabled(rename = "TARGET")]
    target: String,
    #[tabled(rename = "STATUS")]
    status: String,
    #[tabled(rename = "PATH")]
    path: String,
}

#[derive(Tabled)]
struct OutdatedRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "TYPE")]
    capability_type: String,
    #[tabled(rename = "TARGET")]
    target: String,
    #[tabled(rename = "CURRENT")]
    current: String,
    #[tabled(rename = "LATEST")]
    latest: String,
    #[tabled(rename = "STATUS")]
    status: String,
}

fn use_color() -> bool {
    std::env::var_os("NO_COLOR").is_none()
}

fn paint(text: &str, code: &str) -> String {
    if use_color() {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

fn style_drift_status(status: &str) -> String {
    match status {
        "clean" => format!("{} {}", paint("✓", "32"), paint("clean", "32")),
        "modified" => format!("{} {}", paint("●", "33"), paint("modified", "33")),
        "missing" => format!("{} {}", paint("✗", "31"), paint("missing", "31")),
        "error" => format!("{} {}", paint("✗", "31"), paint("error", "31")),
        other => other.to_string(),
    }
}

fn style_outdated_status(status: &str) -> String {
    match status {
        "up to date" => format!("{} {}", paint("✓", "32"), paint("up to date", "32")),
        "outdated" => format!("{} {}", paint("●", "33"), paint("outdated", "33")),
        "error" => format!("{} {}", paint("✗", "31"), paint("error", "31")),
        other => other.to_string(),
    }
}

pub fn cmd_init(repo_root: &Path, global: bool) -> Result<()> {
    if global {
        display::print_init_banner();
        let home = home_dir()?;
        let lock_path = home.join(".coral").join("coral-lock.json");
        lockfile::init_lockfile_at(&lock_path)?;
        println!("initialized ~/.coral/coral-lock.json");
    } else {
        display::print_init_banner();
        let lock_path = lockfile::init_lockfile(repo_root)?;
        let _ = config::read_config(repo_root)?;

        // Scaffold agent directories (smart skip: only create missing ones)
        for dir in &["skills", "tools", "hooks", "workflows"] {
            let path = repo_root.join(".agents").join(dir);
            if !path.exists() {
                std::fs::create_dir_all(&path)?;
            }
        }

        println!(
            "initialized {}",
            lockfile::relative_or_absolute_fs(&lock_path, repo_root)
        );
        println!("scaffolded .agents/ — place your capabilities here:");

        // Install coral-cli-guide skill
        let guide_path = repo_root.join(".agents").join("skills").join("coral-cli-guide");
        if !guide_path.exists() {
            std::fs::create_dir_all(&guide_path)?;
            std::fs::write(guide_path.join("SKILL.md"), CORAL_GUIDE_CONTENT)?;

            // Baseline
            let baseline_dir = repo_root
                .join(".coral")
                .join("baselines")
                .join("open-agents")
                .join("coral-cli-guide");
            std::fs::create_dir_all(&baseline_dir)?;
            std::fs::write(baseline_dir.join("SKILL.md"), CORAL_GUIDE_CONTENT)?;

            // Lockfile entry
            let hash = lockfile::hash_bytes(CORAL_GUIDE_CONTENT.as_bytes());
            let mut lf = lockfile::require_lockfile(repo_root).unwrap_or(lockfile::Lockfile {
                version: lockfile::LOCKFILE_VERSION,
                capabilities: BTreeMap::new(),
            });

            let mut targets = BTreeMap::new();
            targets.insert(
                "open-agents".to_string(),
                lockfile::TargetLockEntry {
                    baseline_dir: ".coral/baselines/open-agents/coral-cli-guide".into(),
                    emitted_files: vec![adapter::EmittedFile {
                        path: ".agents/skills/coral-cli-guide/SKILL.md".into(),
                        hash,
                    }],
                },
            );

            lf.capabilities.insert(
                "coral-cli-guide".into(),
                lockfile::CapabilityLockEntry {
                    capability_type: "skill".into(),
                    installed_version: "0.1.0".into(),
                    source_path: String::new(),
                    targets,
                    source: None,
                    scope: "project".into(),
                },
            );

            lockfile::write_lockfile(repo_root, &lf)?;
        }
    }
    Ok(())
}

pub fn cmd_create(
    repo_root: &Path,
    skill: Option<&str>,
    tool: Option<&str>,
    hook: Option<&str>,
    workflow: Option<&str>,
) -> Result<()> {
    let selections = [
        ("skill", skill),
        ("tool", tool),
        ("hook", hook),
        ("workflow", workflow),
    ];
    let chosen: Vec<_> = selections
        .iter()
        .filter_map(|(kind, value)| value.map(|name| (*kind, name)))
        .collect();

    if chosen.len() != 1 {
        return Err(CoralError::new(
            "choose exactly one scaffold target: --skill, --tool, --hook, or --workflow",
        ));
    }

    let (kind, raw_name) = chosen[0];
    let id = raw_name.trim();
    if id.is_empty() {
        return Err(CoralError::new("capability name must not be empty"));
    }

    let (relative_dir, files): (&str, Vec<(&str, String)>) = match kind {
        "skill" => (
            "skills",
            vec![
                (
                    "coral.toml",
                    format!(
                        r#"id = "{id}"
version = "0.1.0"
type = "skill"
description = "What this skill helps the agent do."
files = ["SKILL.md"]
"#
                    ),
                ),
                (
                    "SKILL.md",
                    format!(
                        r#"# {id}

## Purpose

Describe when the agent should use this skill.

## Guidance

- Add the key rules the agent should follow.
- Add examples, constraints, and team conventions.
"#
                    ),
                ),
            ],
        ),
        "tool" => (
            "tools",
            vec![
                (
                    "coral.toml",
                    format!(
                        r#"id = "{id}"
version = "0.1.0"
type = "tool"
description = "What this tool does for the agent."
files = ["run.sh"]

[parameters]
type = "object"

[parameters.properties.input]
type = "string"
description = "Input passed to the tool"

[implementation]
language = "bash"
entrypoint = "run.sh"
"#
                    ),
                ),
                (
                    "run.sh",
                    r#"#!/usr/bin/env bash
set -euo pipefail

echo "replace with tool logic"
"#
                    .to_string(),
                ),
            ],
        ),
        "hook" => (
            "hooks",
            vec![(
                "coral.toml",
                format!(
                    r#"id = "{id}"
version = "0.1.0"
type = "hook"
description = "What this hook enforces."
files = ["hook.toml"]

[hook]
event = "before_finish"
command = "echo review hook"
working_directory = "."
"#
                ),
            ), (
                "hook.toml",
                "event = \"before_finish\"\ncommand = \"echo review hook\"\n".to_string(),
            )],
        ),
        "workflow" => (
            "workflows",
            vec![(
                "coral.toml",
                format!(
                    r#"id = "{id}"
version = "0.1.0"
type = "workflow"
description = "When the agent should run this workflow."
files = ["workflow.toml"]

[[workflow.requires]]
id = "replace-me"
type = "skill"
"#
                ),
            ), (
                "workflow.toml",
                "name = \"replace-me\"\ndescription = \"Fill in the workflow steps here.\"\n".to_string(),
            )],
        ),
        _ => unreachable!(),
    };

    let root = repo_root.join(".agents").join(relative_dir).join(id);
    if root.exists() {
        return Err(CoralError::new(format!(
            "refusing to overwrite existing capability scaffold: {}",
            lockfile::relative_or_absolute_fs(&root, repo_root)
        )));
    }

    fs::create_dir_all(&root)?;
    for (name, content) in files {
        fs::write(root.join(name), content)?;
    }

    #[cfg(unix)]
    if kind == "tool" {
        use std::os::unix::fs::PermissionsExt;
        let run_sh = root.join("run.sh");
        let mut permissions = fs::metadata(&run_sh)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(run_sh, permissions)?;
    }

    println!(
        "created {kind} scaffold at {}",
        lockfile::relative_or_absolute_fs(&root, repo_root)
    );
    println!(
        "next: edit the generated files, then run `coral import {} -t <target>`",
        lockfile::relative_or_absolute_fs(&root, repo_root)
    );

    Ok(())
}

// ── Add ─────────────────────────────────────────────────────────────────────

pub fn cmd_add(
    repo_root: &Path,
    capability: &Path,
    target_ids: &[String],
    skill_name: Option<&str>,
    tool_name: Option<&str>,
    hook_name: Option<&str>,
    global: bool,
) -> Result<()> {
    let (scope, install_root) = if global {
        let home = home_dir()?;
        let lock_path = home.join(".coral").join("coral-lock.json");
        lockfile::init_lockfile_at(&lock_path)?;
        (Scope::Global, home)
    } else {
        (Scope::Project, repo_root.to_path_buf())
    };

    if git::is_git_url(&capability.to_string_lossy()) {
        return cmd_add_git(
            &install_root,
            scope,
            &capability.to_string_lossy(),
            target_ids,
            skill_name,
            tool_name,
            hook_name,
            repo_root,
        );
    }
    cmd_add_local(&install_root, scope, capability, target_ids, repo_root)
}

fn cmd_add_git(
    install_root: &Path,
    scope: Scope,
    url: &str,
    target_ids: &[String],
    skill_name: Option<&str>,
    tool_name: Option<&str>,
    hook_name: Option<&str>,
    project_root: &Path,
) -> Result<()> {
    let name = skill_name.or(tool_name).or(hook_name).ok_or_else(|| {
        CoralError::new("--skill, --tool, or --hook is required when installing from a git URL")
    })?;

    let (cache_dir, clean_url) = git::clone_or_fetch(url)?;
    let commit_sha = git::resolve_ref(&cache_dir)?;
    let skill_dir = git::discover_skill(&cache_dir, name)?;

    let manifest = manifest::synthetic_manifest(&skill_dir, name, &commit_sha)?;
    let capability = resolve_capability(&manifest)?;

    if scope == Scope::Project {
        if let Some(warning) = resolver::check_collision(name, project_root, Some(&clean_url))? {
            eprintln!("{warning}");
        }
    }

    install_capability(install_root, scope, &capability, &manifest, target_ids, Some(&SourceMetaInput {
        source_type: "git".to_string(),
        url: clean_url,
        source_ref: commit_sha.clone(),
        skill: name.to_string(),
    }))
}

fn cmd_add_local(
    install_root: &Path,
    scope: Scope,
    capability: &Path,
    target_ids: &[String],
    project_root: &Path,
) -> Result<()> {
    let capability_dir = lockfile::absolutize(install_root, capability);
    let capability = load_manifest(&capability_dir)?;
    let resolved = resolve_capability(&capability)?;

    if scope == Scope::Project {
        if let Some(warning) = resolver::check_collision(&capability.id, project_root, None)? {
            eprintln!("{warning}");
        }
    }

    install_capability(install_root, scope, &resolved, &capability, target_ids, None)
}

struct SourceMetaInput {
    source_type: String,
    url: String,
    source_ref: String,
    skill: String,
}

fn install_capability(
    install_root: &Path,
    scope: Scope,
    capability: &adapter::ResolvedCapability,
    manifest: &manifest::CapabilityManifest,
    target_ids: &[String],
    source_meta: Option<&SourceMetaInput>,
) -> Result<()> {
    let is_git = source_meta.is_some();

    let mut adapters = Vec::new();
    for tid in target_ids {
        let adapter = AdapterKind::from_id(tid).ok_or_else(|| {
            CoralError::new(format!(
                "unknown target '{}'; run 'coral target list' to see available targets",
                tid
            ))
        })?;
        if !adapter.supports(&capability.capability_type) {
            return Err(CoralError::new(format!(
                "{} does not yet support {} capabilities",
                adapter.display_name(),
                capability.capability_type
            )));
        }
        if capability.capability_type == "hook" {
            if let Some(ref hook_cfg) = capability.hook {
                if !adapter.supported_events().contains(&hook_cfg.event.as_str()) {
                    return Err(CoralError::new(format!(
                        "{} does not support hook event '{}'. Supported events: {}",
                        adapter.display_name(),
                        hook_cfg.event,
                        adapter.supported_events().join(", ")
                    )));
                }
            }
        }
        adapters.push(adapter);
    }

    let mut plans: Vec<(AdapterKind, Vec<adapter::PlannedFile>)> = Vec::new();
    for adapter in &adapters {
        let planned = adapter.plan(&capability, install_root)?;
        plans.push((*adapter, planned));
    }

    let lockfile = lockfile::require_lockfile(install_root)?;
    for (adapter, planned_files) in &plans {
        let is_tracked = lockfile
            .capabilities
            .get(&capability.id)
            .and_then(|e| e.targets.get(adapter.id()))
            .is_some();
        if !is_tracked {
            for f in planned_files {
                let target_path = install_root.join(&f.path);
                if target_path.exists() {
                    return Err(CoralError::new(format!(
                        "refusing to overwrite untracked file at {}; remove it or track it in Coral first",
                        lockfile::relative_or_absolute_fs(&target_path, install_root)
                    )));
                }
            }
        }
    }

    let mut new_targets: BTreeMap<String, TargetLockEntry> = BTreeMap::new();

    for (adapter, planned_files) in &plans {
        let mut emitted = Vec::new();

        for planned in planned_files {
            let target_path = install_root.join(&planned.path);
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target_path, &planned.content)?;

            let hash = lockfile::hash_bytes(&planned.content);
            emitted.push(adapter::EmittedFile {
                path: planned.path.clone(),
                hash,
            });

            println!(
                "installed {} ({}) -> {}",
                capability.id,
                adapter.id(),
                lockfile::relative_or_absolute_fs(&target_path, install_root)
            );
        }

        let baseline_dir = install_root
            .join(".coral")
            .join("baselines")
            .join(adapter.id())
            .join(&capability.id);
        std::fs::create_dir_all(&baseline_dir)?;

        for planned in planned_files {
            let file_name = Path::new(&planned.path)
                .file_name()
                .expect("emitted file should have a name");
            std::fs::write(baseline_dir.join(file_name), &planned.content)?;
        }

        let baseline_rel = lockfile::relative_or_absolute_fs(&baseline_dir, install_root);
        new_targets.insert(
            adapter.id().to_string(),
            TargetLockEntry {
                baseline_dir: baseline_rel,
                emitted_files: emitted,
            },
        );
    }

    // MCP registration for tool primitives
    if capability.capability_type == "tool" {
        if let Some(ref impl_cfg) = capability.implementation {
            for adapter in &adapters {
                let mcp_path = match adapter {
                    AdapterKind::OpenAgents => {
                        install_root.join(".agents").join("mcp.json")
                    }
                    AdapterKind::Claude => {
                        install_root.join(".mcp.json")
                    }
                };

                let mcp_command = impl_cfg.language.clone();
                let entrypoint_path = match adapter {
                    AdapterKind::OpenAgents => {
                        format!(".agents/tools/{}/{}", capability.id, impl_cfg.entrypoint)
                    }
                    AdapterKind::Claude => {
                        format!(".claude/tools/{}/{}", capability.id, impl_cfg.entrypoint)
                    }
                };
                let mcp_args = vec![entrypoint_path];

                crate::adapters::mcp_register_tool(
                    install_root,
                    &mcp_path,
                    &capability.id,
                    &mcp_command,
                    &mcp_args,
                )?;
            }
        }
    }

    let mut lockfile = lockfile;
    let existing_targets = lockfile
        .capabilities
        .get(&capability.id)
        .map(|e| e.targets.clone())
        .unwrap_or_default();

    let mut merged_targets = existing_targets;
    for (k, v) in new_targets {
        merged_targets.insert(k, v);
    }

    let source_path = if is_git {
        String::new()
    } else {
        lockfile::relative_or_absolute_fs(&manifest.root, install_root)
    };

    lockfile.capabilities.insert(
        capability.id.clone(),
        lockfile::CapabilityLockEntry {
            capability_type: capability.capability_type.clone(),
            installed_version: capability.version.clone(),
            source_path,
            targets: merged_targets,
            source: source_meta.map(|m| lockfile::SourceMetadata {
                source_type: m.source_type.clone(),
                url: m.url.clone(),
                source_ref: m.source_ref.clone(),
                skill: m.skill.clone(),
            }),
            scope: scope.as_str().to_string(),
        },
    );

    lockfile::write_lockfile(install_root, &lockfile)?;
    Ok(())
}

// ── List ────────────────────────────────────────────────────────────────────

pub fn cmd_list(repo_root: &Path, scope_filter: &str, kind_filter: Option<&str>) -> Result<()> {
    let show_project = scope_filter == "all" || scope_filter == "project";
    let show_global = scope_filter == "all" || scope_filter == "global";

    let mut rows: Vec<ListRow> = Vec::new();

    if show_project {
        if let Ok(lockfile) = lockfile::require_lockfile(repo_root) {
            for (id, entry) in &lockfile.capabilities {
                if let Some(kind) = kind_filter {
                    if entry.capability_type != kind {
                        continue;
                    }
                }
                for (target_id, target_entry) in &entry.targets {
                    for emitted in &target_entry.emitted_files {
                        let status = lockfile::drift_status(repo_root, emitted);
                        rows.push(ListRow {
                            id: id.clone(),
                            capability_type: entry.capability_type.clone(),
                            version: short_sha(&entry.installed_version).to_string(),
                            scope: "project".to_string(),
                            target: target_id.clone(),
                            status: style_drift_status(status),
                            path: emitted.path.clone(),
                        });
                    }
                }
            }
        }
    }

    if show_global {
        if let Some(home) = home_dir_opt() {
            let lock_path = home.join(".coral").join("coral-lock.json");
            if let Ok(lockfile) = lockfile::read_lockfile_at(&lock_path) {
                for (id, entry) in &lockfile.capabilities {
                    if let Some(kind) = kind_filter {
                        if entry.capability_type != kind {
                            continue;
                        }
                    }
                    for (target_id, target_entry) in &entry.targets {
                        for emitted in &target_entry.emitted_files {
                            let status = lockfile::drift_status(&home, emitted);
                            rows.push(ListRow {
                                id: id.clone(),
                                capability_type: entry.capability_type.clone(),
                                version: short_sha(&entry.installed_version).to_string(),
                                scope: "global".to_string(),
                                target: target_id.clone(),
                                status: style_drift_status(status),
                                path: format!("~/{}", emitted.path),
                            });
                        }
                    }
                }
            }
        }
    }

    if rows.is_empty() {
        println!("no capabilities installed");
        return Ok(());
    }

    rows.sort_by(|a, b| a.scope.cmp(&b.scope).then_with(|| a.id.cmp(&b.id)));

    let mut table = Table::new(rows);
    table
        .with(Style::rounded())
        .with(Modify::new(Columns::single(6)).with(Width::wrap(42).keep_words(true)));
    println!("{table}");
    Ok(())
}

// ── Status ──────────────────────────────────────────────────────────────────

pub fn cmd_status(repo_root: &Path) -> Result<()> {
    let mut found_any = false;

    if let Ok(lockfile) = lockfile::require_lockfile(repo_root) {
        for (id, entry) in &lockfile.capabilities {
            let mut flags = Vec::new();
            for (_, target_entry) in &entry.targets {
                for emitted in &target_entry.emitted_files {
                    let s = lockfile::drift_status(repo_root, emitted);
                    if s != "clean" {
                        flags.push(s);
                    }
                }
            }

            let override_warning = if resolver::overrides_global(id, repo_root).unwrap_or(false) {
                " [overrides global — won't receive global updates]"
            } else {
                ""
            };

            let drift = if flags.is_empty() {
                "clean".to_string()
            } else {
                flags.join(",")
            };

            println!("{id}  project  {drift}{override_warning}");

            if entry.capability_type == "workflow" && !entry.targets.is_empty() {
                let (_, target_entry) = entry.targets.iter().next().unwrap();
                for emitted in &target_entry.emitted_files {
                    if let Ok(content) = std::fs::read_to_string(repo_root.join(&emitted.path)) {
                        let mut requires = Vec::new();
                        let mut current_id = String::new();
                        for line in content.lines() {
                            if line.starts_with("id = \"") && !line.contains("version") && !line.contains("description") && !line.contains("type") {
                                current_id = line.trim_start_matches("id = \"").trim_end_matches('"').to_string();
                            }
                            if line.starts_with("type = \"") {
                                let ctype = line.trim_start_matches("type = \"").trim_end_matches('"').to_string();
                                if !current_id.is_empty() && ctype != "workflow" {
                                    requires.push((current_id.clone(), ctype));
                                    current_id.clear();
                                }
                            }
                        }

                        for (req_id, req_type) in &requires {
                            let child_status = if let Ok(lf) = lockfile::require_lockfile(repo_root) {
                                if let Some(child_entry) = lf.capabilities.get(req_id) {
                                    let mut child_drift = Vec::new();
                                    for (_, ct_entry) in &child_entry.targets {
                                        for e in &ct_entry.emitted_files {
                                            let s = lockfile::drift_status(repo_root, e);
                                            if s != "clean" { child_drift.push(s); }
                                        }
                                    }
                                    if child_drift.is_empty() { "clean".to_string() } else { child_drift.join(",") }
                                } else {
                                    "not installed".to_string()
                                }
                            } else {
                                "unknown".to_string()
                            };
                            println!("  ├─ {req_id:<30} {req_type:<12} {child_status}");
                        }
                    }
                    break;
                }
            }

            found_any = true;
        }
    }

    if let Some(home) = home_dir_opt() {
        let lock_path = home.join(".coral").join("coral-lock.json");
        if let Ok(lockfile) = lockfile::read_lockfile_at(&lock_path) {
            for (id, entry) in &lockfile.capabilities {
                let mut flags = Vec::new();
                for (_, target_entry) in &entry.targets {
                    for emitted in &target_entry.emitted_files {
                        let s = lockfile::drift_status(&home, emitted);
                        if s != "clean" {
                            flags.push(s);
                        }
                    }
                }

                let is_shadowed = {
                    lockfile::require_lockfile(repo_root)
                        .map(|plf| plf.capabilities.contains_key(id))
                        .unwrap_or(false)
                };

                let note = if is_shadowed {
                    " [shadowed by project copy]"
                } else {
                    ""
                };

                let drift = if flags.is_empty() {
                    "clean".to_string()
                } else {
                    flags.join(",")
                };

                println!("{id}  global   {drift}{note}");
                found_any = true;
            }
        }
    }

    if !found_any {
        println!("no capabilities installed");
    }

    Ok(())
}

// ── Diff ────────────────────────────────────────────────────────────────────

pub fn cmd_diff(
    repo_root: &Path,
    capability_id: &str,
    target: Option<&str>,
    upstream: bool,
) -> Result<()> {
    let (scope, entry, scope_root) = match resolver::resolve_entry(capability_id, repo_root)? {
        Some((s, e)) => {
            let root = match s {
                Scope::Project => repo_root.to_path_buf(),
                Scope::Global => home_dir()?,
            };
            (s, e, root)
        }
        None => {
            return Err(CoralError::new(format!(
                "capability is not installed: {capability_id}"
            )));
        }
    };

    let _ = scope;

    if upstream {
        return cmd_diff_upstream(scope_root, capability_id, &entry, target);
    }

    let mut output = String::new();

    if let Some(tid) = target {
        output.push_str(&lockfile::diff_against_baseline(
            &scope_root,
            capability_id,
            tid,
            &entry,
        )?);
    } else {
        let mut first = true;
        for tid in entry.targets.keys() {
            let diff = lockfile::diff_against_baseline(&scope_root, capability_id, tid, &entry)?;
            if diff.is_empty() {
                continue;
            }
            if !first {
                output.push('\n');
            }
            output.push_str(&diff);
            first = false;
        }
    }

    print!("{output}");
    Ok(())
}

fn cmd_diff_upstream(
    scope_root: PathBuf,
    _capability_id: &str,
    entry: &lockfile::CapabilityLockEntry,
    target: Option<&str>,
) -> Result<()> {
    let source = entry.source.as_ref().ok_or_else(|| {
        CoralError::new("upstream diff only available for git-sourced primitives")
    })?;

    let (cache_dir, _) = git::clone_or_fetch(&source.url)?;
    let mut output = String::new();

    for (tid, target_entry) in &entry.targets {
        if let Some(t) = target {
            if tid != t {
                continue;
            }
        }

        let baseline_dir = scope_root.join(&target_entry.baseline_dir);
        for emitted in &target_entry.emitted_files {
            let file_name = std::path::Path::new(&emitted.path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();

            let upstream_content = crate::diff::get_upstream_content(
                &cache_dir,
                &source.skill,
                &file_name,
            )?;
            let baseline_path = baseline_dir.join(file_name.as_ref());
            let baseline_content = std::fs::read_to_string(&baseline_path)?;

            if baseline_content == upstream_content {
                continue;
            }

            output.push_str(&format!(
                "--- baseline/{}\n+++ upstream/{}/{}\n",
                file_name, tid, file_name
            ));
            let diff =
                similar::TextDiff::from_lines(&baseline_content, &upstream_content);
            for group in diff.grouped_ops(3) {
                for operation in group {
                    for change in diff.iter_changes(&operation) {
                        let sign = match change.tag() {
                            similar::ChangeTag::Delete => "-",
                            similar::ChangeTag::Insert => "+",
                            similar::ChangeTag::Equal => " ",
                        };
                        output.push_str(sign);
                        output.push_str(change.value());
                    }
                }
            }
        }
    }

    if output.is_empty() {
        println!("no upstream changes");
    } else {
        print!("{output}");
    }

    Ok(())
}

// ── Remove ──────────────────────────────────────────────────────────────────

pub fn cmd_remove(
    repo_root: &Path,
    id: &str,
    scope_str: &str,
    targets: Option<&[String]>,
) -> Result<()> {
    let scope = resolver::Scope::from_str(scope_str)
        .ok_or_else(|| CoralError::new(format!("invalid scope '{}'", scope_str)))?;

    let scope_root = match scope {
        Scope::Project => repo_root.to_path_buf(),
        Scope::Global => home_dir()?,
    };

    let mut lockfile = lockfile::require_lockfile(&scope_root)?;
    let entry = lockfile.capabilities.remove(id).ok_or_else(|| {
        CoralError::new(format!("'{}' is not installed in {} scope", id, scope.as_str()))
    })?;

    let mut modified = false;
    for (_, target_entry) in &entry.targets {
        for emitted in &target_entry.emitted_files {
            if lockfile::drift_status(&scope_root, emitted) == "modified" {
                modified = true;
            }
        }
    }
    if modified {
        eprintln!("warning: '{}' has local modifications — removing will discard them", id);
    }

    let adapter_ids: Vec<String> = if let Some(tgts) = targets {
        tgts.to_vec()
    } else {
        entry.targets.keys().cloned().collect()
    };

    for tid in &adapter_ids {
        if let Some(adapter) = AdapterKind::from_id(tid) {
            adapter.remove(id, &scope_root)?;

            let baseline_dir = scope_root
                .join(".coral")
                .join("baselines")
                .join(tid)
                .join(id);
            if baseline_dir.exists() {
                std::fs::remove_dir_all(&baseline_dir)?;
            }
        }
    }

    lockfile::write_lockfile(&scope_root, &lockfile)?;
    println!("removed '{}' from {} scope", id, scope.as_str());
    Ok(())
}

// ── Update ──────────────────────────────────────────────────────────────────

pub fn cmd_update(
    repo_root: &Path,
    id: &str,
    scope_str: Option<&str>,
    check: bool,
    force: bool,
) -> Result<()> {
    let (scope, entry, scope_root) = if let Some(s) = scope_str {
        let scope = resolver::Scope::from_str(s)
            .ok_or_else(|| CoralError::new(format!("invalid scope '{}'", s)))?;
        let root = match scope {
            Scope::Project => repo_root.to_path_buf(),
            Scope::Global => home_dir()?,
        };
        let lf = lockfile::require_lockfile(&root)?;
        let entry = lf.capabilities.get(id).ok_or_else(|| {
            CoralError::new(format!(
                "'{}' is not installed in {} scope",
                id,
                scope.as_str()
            ))
        })?;
        (scope, entry.clone(), root)
    } else {
        match resolver::resolve_entry(id, repo_root)? {
            Some((s, e)) => {
                let root = match s {
                    Scope::Project => repo_root.to_path_buf(),
                    Scope::Global => home_dir()?,
                };
                (s, e, root)
            }
            None => {
                return Err(CoralError::new(format!("'{}' is not installed", id)));
            }
        }
    };

    let source = entry.source.as_ref().ok_or_else(|| {
        CoralError::new(
            format!("'{}' is not a git-sourced capability — update only works for git sources", id),
        )
    })?;

    let (cache_dir, _clean_url) = git::clone_or_fetch(&source.url)?;
    let latest_sha = git::resolve_ref(&cache_dir)?;

    if latest_sha == entry.installed_version {
        println!("'{}' is already up to date", id);
        return Ok(());
    }

    let skill_dir = git::discover_skill(&cache_dir, &source.skill)?;

    if force {
        let manifest = manifest::synthetic_manifest(&skill_dir, id, &latest_sha)?;
        let capability = resolve_capability(&manifest)?;
        let target_ids: Vec<String> = entry.targets.keys().cloned().collect();
        return install_capability(
            &scope_root,
            scope,
            &capability,
            &manifest,
            &target_ids,
            Some(&SourceMetaInput {
                source_type: "git".to_string(),
                url: source.url.clone(),
                source_ref: latest_sha.clone(),
                skill: source.skill.clone(),
            }),
        );
    }

    let mut all_clean = true;
    let mut had_conflicts = false;
    let mut would_overwrite = false;

    for (_tid, target_entry) in &entry.targets {
        let baseline_dir = scope_root.join(&target_entry.baseline_dir);
        for emitted in &target_entry.emitted_files {
            let file_name = std::path::Path::new(&emitted.path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();

            let local_path = scope_root.join(&emitted.path);
            let baseline_path = baseline_dir.join(&*file_name);
            let upstream_path = skill_dir.join(&*file_name);

            let local_content = std::fs::read_to_string(&local_path).unwrap_or_default();
            let baseline_content = std::fs::read_to_string(&baseline_path).unwrap_or_default();
            let upstream_content = std::fs::read_to_string(&upstream_path).unwrap_or_default();

            if local_content == upstream_content {
                continue;
            }

            let local_status = lockfile::drift_status(&scope_root, emitted);
            if local_status != "clean" || local_content != baseline_content {
                all_clean = false;
                would_overwrite = true;
            }

            if !check {
                let report = crate::diff::merge_and_write(
                    &baseline_path,
                    &local_path,
                    &upstream_path,
                )?;

                if let Some(reports) = report {
                    had_conflicts = true;
                    for r in reports {
                        for c in &r.conflicts {
                            eprintln!("  ✗ {}: {}", r.file_path, c.description);
                            eprintln!("    <<<<<< local\n{}\n    ======", c.local.trim());
                            eprintln!("    {}\n    >>>>>> upstream", c.upstream.trim());
                        }
                    }
                    eprintln!(
                        "\n  To write conflict markers: coral update {} --write-conflicts",
                        id
                    );
                }
            }
        }
    }

    if check {
        if all_clean {
            println!("'{}' can be updated cleanly (no local changes)", id);
        } else if would_overwrite {
            println!("'{}' has local changes — update would attempt three-way merge", id);
        } else {
            println!("'{}' is up to date", id);
        }
        return Ok(());
    }

    if had_conflicts {
        return Err(CoralError::new(
            "conflicts found — local files have not been modified. Resolve conflicts manually or use --force to overwrite.",
        ));
    }

    let manifest = manifest::synthetic_manifest(&skill_dir, id, &latest_sha)?;
    let primitive = resolve_capability(&manifest)?;
    let target_ids: Vec<String> = entry.targets.keys().cloned().collect();

    install_capability(
        &scope_root,
        scope,
        &primitive,
        &manifest,
        &target_ids,
        Some(&SourceMetaInput {
            source_type: "git".to_string(),
            url: source.url.clone(),
            source_ref: latest_sha.clone(),
            skill: source.skill.clone(),
        }),
    )
}

// ── Import ──────────────────────────────────────────────────────────────────

pub fn cmd_import(
    repo_root: &Path,
    path: Option<&Path>,
    targets: &[String],
    capability_type: Option<&str>,
    dry_run: bool,
    override_existing: bool,
) -> Result<()> {
    let mut import_paths: Vec<(std::path::PathBuf, String, String)> = Vec::new();

    if let Some(dir) = path {
        let dir = lockfile::absolutize(repo_root, dir);
        if !dir.exists() || !dir.is_dir() {
            return Err(CoralError::new(format!(
                "directory not found: {}",
                dir.display()
            )));
        }

        let (inferred_type, inferred_target) = infer_from_path(&dir);
        let ctype = capability_type.unwrap_or(&inferred_type);
        let tid = if targets.is_empty() {
            inferred_target
        } else {
            targets[0].clone()
        };

        import_paths.push((dir, ctype.to_string(), tid));
    } else {
        // Batch scan
        if targets.is_empty() {
            // Scan both default locations
            for (base, target_id) in &[
                (".agents", "open-agents"),
                (".claude", "claude"),
            ] {
                let base_dir = repo_root.join(base);
                if base_dir.exists() {
                    scan_agent_dir(&base_dir, target_id, &mut import_paths)?;
                }
            }
        } else {
            for tid in targets {
                let base = if tid == "open-agents" { ".agents" } else { ".claude" };
                let base_dir = repo_root.join(base);
                if base_dir.exists() {
                    scan_agent_dir(&base_dir, tid, &mut import_paths)?;
                }
            }
        }
    }

    if import_paths.is_empty() {
        println!("no assets found to import");
        return Ok(());
    }

    for (dir, ctype, target_id) in &import_paths {
        let id = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        if dry_run {
            println!(
                "would import {} ({}, {}) from {}",
                id, ctype, target_id, lockfile::relative_or_absolute_fs(dir, repo_root)
            );
            continue;
        }

        // Check if already in lockfile
        if !override_existing {
            if let Ok(lf) = lockfile::require_lockfile(repo_root) {
                if lf.capabilities.contains_key(&id) {
                    println!("skipped {} — already tracked (use --override to overwrite)", id);
                    continue;
                }
            }
        }

        // Generate coral.toml
        let toml_content = format!(
            "# Generated by coral import\nid = \"{id}\"\nversion = \"0.1.0\"\ntype = \"{ctype}\"\ndescription = \"Imported from existing agent assets.\"\n",
            id = id,
            ctype = ctype
        );
        std::fs::write(dir.join("coral.toml"), toml_content)?;

        // Build file list
        let mut file_list = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let fname = entry.file_name();
                if fname == "coral.toml" {
                    continue;
                }
                file_list.push(fname.to_string_lossy().to_string());
            }
        }

        // Create manifest and import
        let manifest = manifest::CapabilityManifest {
            id: id.clone(),
            version: "0.1.0".into(),
            capability_type: ctype.clone(),
            description: "Imported from existing agent assets.".into(),
            files: file_list,
            parameters: None,
            implementation: None,
            hook: None,
            workflow: None,
            targets: vec![],
            root: dir.clone(),
        };

        let capability = resolve_capability(&manifest)?;

        // Import: files already exist — skip emit, just baseline + lockfile
        let entrypoint_files: Vec<String> = capability
            .source_files
            .iter()
            .map(|(name, _)| name.clone())
            .collect();

        if !entrypoint_files.is_empty() {
            let mut lockfile = lockfile::require_lockfile(repo_root)?;
            let mut emitted_files = Vec::new();

            let baseline_dir = repo_root
                .join(".coral")
                .join("baselines")
                .join(target_id)
                .join(&id);
            std::fs::create_dir_all(&baseline_dir)?;

            for (rel_path, content) in &capability.source_files {
                let file_path = dir.join(rel_path);
                let hash = lockfile::hash_bytes(&std::fs::read(&file_path)?);
                emitted_files.push(adapter::EmittedFile {
                    path: lockfile::relative_or_absolute_fs(&file_path, repo_root),
                    hash,
                });
                std::fs::write(baseline_dir.join(rel_path), content)?;
            }

            let baseline_rel = lockfile::relative_or_absolute_fs(&baseline_dir, repo_root);
            let mut targets = BTreeMap::new();
            targets.insert(
                target_id.clone(),
                lockfile::TargetLockEntry {
                    baseline_dir: baseline_rel,
                    emitted_files,
                },
            );

            lockfile.capabilities.insert(
                id.clone(),
                lockfile::CapabilityLockEntry {
                    capability_type: ctype.clone(),
                    installed_version: "0.1.0".into(),
                    source_path: String::new(),
                    targets,
                    source: None,
                    scope: "project".into(),
                },
            );

            lockfile::write_lockfile(repo_root, &lockfile)?;
            println!(
                "imported {} ({}, {})",
                id, ctype, target_id
            );
        }
    }

    Ok(())
}

fn infer_from_path(path: &Path) -> (String, String) {
    let parent = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let ctype = match parent.as_str() {
        "tools" => "tool",
        "hooks" => "hook",
        _ => "skill",
    };

    let grandparent = path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let target = match grandparent.as_str() {
        ".claude" => "claude",
        _ => "open-agents",
    };

    (ctype.to_string(), target.to_string())
}

fn scan_agent_dir(
    base: &Path,
    target_id: &str,
    results: &mut Vec<(std::path::PathBuf, String, String)>,
) -> Result<()> {
    for kind in &["skills", "tools", "hooks"] {
        let kind_dir = base.join(kind);
        if kind_dir.exists() {
            for entry in std::fs::read_dir(&kind_dir)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    let ctype = kind.trim_end_matches('s');
                    results.push((entry.path(), ctype.to_string(), target_id.to_string()));
                }
            }
        }
    }
    Ok(())
}

// ── Check ───────────────────────────────────────────────────────────────────

pub fn cmd_check(repo_root: &Path, json: bool, ignore_failures: bool, _global: bool) -> Result<()> {
    let outcome = crate::check::run_checks(repo_root)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&outcome)?);
    } else {
        for r in &outcome.results {
            let mark = if r.status == "ok" {
                paint("✓", "32")
            } else {
                paint("✗", "31")
            };
            let status = if r.status == "ok" {
                paint("ok", "32")
            } else {
                paint(&r.status, "31")
            };
            let extra = if !r.files.is_empty() {
                format!(" ({})", r.files.join(", "))
            } else {
                String::new()
            };
            println!(
                "{mark} {:<24} {:<10} {:<12} {}{}",
                r.id, r.capability_type, r.target, status, extra
            );
        }
    }

    if !outcome.valid && !ignore_failures {
        std::process::exit(1);
    }

    Ok(())
}

// ── Outdated ────────────────────────────────────────────────────────────────

fn short_sha(v: &str) -> &str {
    if v.len() >= 7 && v.chars().all(|c| c.is_ascii_hexdigit()) {
        &v[..7]
    } else {
        v
    }
}

pub fn cmd_outdated(repo_root: &Path) -> Result<()> {
    let mut rows: Vec<OutdatedRow> = Vec::new();

    if let Ok(lockfile) = lockfile::require_lockfile(repo_root) {
        for (id, entry) in &lockfile.capabilities {
            for (target_id, _target_entry) in &entry.targets {
                let (current, latest, status) = if let Some(src) = &entry.source {
                    let latest_sha = match git::clone_or_fetch(&src.url)
                        .and_then(|(d, _)| git::resolve_ref(&d))
                    {
                        Ok(sha) => sha,
                        Err(_) => {
                            rows.push(OutdatedRow {
                                id: id.clone(),
                                capability_type: entry.capability_type.clone(),
                                target: target_id.clone(),
                                current: short_sha(&entry.installed_version).to_string(),
                                latest: "unavailable".to_string(),
                                status: "error".to_string(),
                            });
                            continue;
                        }
                    };
                    if latest_sha == entry.installed_version {
                        (
                            short_sha(&entry.installed_version).to_string(),
                            short_sha(&latest_sha).to_string(),
                            "up to date".to_string(),
                        )
                    } else {
                        (
                            short_sha(&entry.installed_version).to_string(),
                            short_sha(&latest_sha).to_string(),
                            "outdated".to_string(),
                        )
                    }
                } else {
                    (
                        entry.installed_version.clone(),
                        "—".to_string(),
                        "up to date".to_string(),
                    )
                };

                rows.push(OutdatedRow {
                    id: id.clone(),
                    capability_type: entry.capability_type.clone(),
                    target: target_id.clone(),
                    current,
                    latest,
                    status,
                });
            }
        }
    }

    if let Some(home) = home_dir_opt() {
        let lock_path = home.join(".coral").join("coral-lock.json");
        if let Ok(lockfile) = lockfile::read_lockfile_at(&lock_path) {
            for (id, entry) in &lockfile.capabilities {
                for (target_id, _target_entry) in &entry.targets {
                    let (current, latest, status) = if let Some(src) = &entry.source {
                        let latest_sha = match git::clone_or_fetch(&src.url)
                            .and_then(|(d, _)| git::resolve_ref(&d))
                        {
                            Ok(sha) => sha,
                            Err(_) => {
                                rows.push(OutdatedRow {
                                    id: id.clone(),
                                    capability_type: entry.capability_type.clone(),
                                    target: target_id.clone(),
                                    current: short_sha(&entry.installed_version)
                                        .to_string(),
                                    latest: "unavailable".to_string(),
                                    status: "error".to_string(),
                                });
                                continue;
                            }
                        };
                        if latest_sha == entry.installed_version {
                            (
                                short_sha(&entry.installed_version).to_string(),
                                short_sha(&latest_sha).to_string(),
                                "up to date".to_string(),
                            )
                        } else {
                            (
                                short_sha(&entry.installed_version).to_string(),
                                short_sha(&latest_sha).to_string(),
                                "outdated".to_string(),
                            )
                        }
                    } else {
                        (
                            entry.installed_version.clone(),
                            "—".to_string(),
                            "up to date".to_string(),
                        )
                    };

                    rows.push(OutdatedRow {
                        id: id.clone(),
                        capability_type: entry.capability_type.clone(),
                        target: target_id.clone(),
                        current,
                        latest,
                        status,
                    });
                }
            }
        }
    }

    if rows.is_empty() {
        println!("no capabilities installed");
        return Ok(());
    }

    rows.sort_by(|a, b| a.id.cmp(&b.id));

    let styled_rows: Vec<OutdatedRow> = rows
        .into_iter()
        .map(|r| {
            let status = style_outdated_status(&r.status);
            OutdatedRow { status, ..r }
        })
        .collect();

    let mut table = Table::new(styled_rows);
    table.with(Style::modern());

    println!("{table}");
    Ok(())
}

// ── Target commands (unchanged) ─────────────────────────────────────────────

pub fn cmd_target_list(repo_root: &Path) -> Result<()> {
    #[derive(Tabled)]
    struct TargetRow {
        #[tabled(rename = "TARGET")]
        target: String,
        #[tabled(rename = "AGENTS SUPPORTED")]
        agents: String,
        #[tabled(rename = "PRIMITIVES")]
        primitives: String,
    }

    let config = config::read_config(repo_root)?;
    let registered: std::collections::HashSet<&str> =
        config.targets.iter().map(|s| s.as_str()).collect();

    let rows: Vec<TargetRow> = AdapterKind::all()
        .iter()
        .map(|a| {
            let target_label = if registered.contains(a.id()) {
                format!("{} *", a.id())
            } else {
                a.id().to_string()
            };
            TargetRow {
                target: target_label,
                agents: a.supported_agents().join(", "),
                primitives: a.kinds_supported().join(", "),
            }
        })
        .collect();

    let mut table = Table::new(rows);
    table
        .with(Style::modern())
        .with(Modify::new(Columns::single(1)).with(
            Width::wrap(40).keep_words(true),
        ));

    println!("{table}");

    if registered.is_empty() {
        println!("\n  *  = registered (use 'coral target add <id>' to register)");
    }
    Ok(())
}

pub fn cmd_target_add(repo_root: &Path, id: &str) -> Result<()> {
    let adapter = AdapterKind::from_id(id).ok_or_else(|| {
        CoralError::new(format!(
            "unknown target '{}'; use 'coral target list' to see available targets",
            id
        ))
    })?;

    let mut config = config::read_config(repo_root)?;
    if config.targets.contains(&id.to_string()) {
        println!("target '{}' is already registered", id);
        return Ok(());
    }

    config.targets.push(id.to_string());
    config::write_config(repo_root, &config)?;
    println!(
        "registered target '{}' ({})",
        adapter.id(),
        adapter.display_name()
    );
    Ok(())
}

pub fn cmd_target_remove(repo_root: &Path, id: &str) -> Result<()> {
    let adapter = AdapterKind::from_id(id).ok_or_else(|| {
        CoralError::new(format!(
            "unknown target '{}'; use 'coral target list' to see available targets",
            id
        ))
    })?;

    let mut config = config::read_config(repo_root)?;
    let was_registered = config.targets.contains(&id.to_string());

    let mut lockfile = lockfile::require_lockfile(repo_root)?;
    for (primitive_id, entry) in lockfile.capabilities.iter() {
        if entry.targets.contains_key(id) {
            adapter.remove(primitive_id, repo_root)?;
            let baseline_dir = repo_root
                .join(".coral")
                .join("baselines")
                .join(id)
                .join(primitive_id);
            if baseline_dir.exists() {
                std::fs::remove_dir_all(&baseline_dir)?;
            }
        }
    }

    for entry in lockfile.capabilities.values_mut() {
        entry.targets.remove(id);
    }

    let baseline_parent = repo_root.join(".coral").join("baselines").join(id);
    if baseline_parent.exists() {
        let is_empty = match std::fs::read_dir(&baseline_parent) {
            Ok(mut rd) => rd.next().is_none(),
            Err(_) => false,
        };
        if is_empty {
            std::fs::remove_dir(&baseline_parent)?;
        }
    }

    lockfile::write_lockfile(repo_root, &lockfile)?;

    config.targets.retain(|t| t != id);
    config::write_config(repo_root, &config)?;

    if was_registered {
        println!("unregistered target '{}'", adapter.id());
    } else {
        println!("removed target '{}'", adapter.id());
    }
    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn home_dir() -> Result<std::path::PathBuf> {
    home_dir_opt().ok_or_else(|| CoralError::new("HOME environment variable not set"))
}

fn home_dir_opt() -> Option<std::path::PathBuf> {
    std::env::var("HOME").ok().map(std::path::PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_sha_truncates_hex() {
        let sha = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0";
        assert_eq!(short_sha(sha), "a1b2c3d");
    }

    #[test]
    fn short_sha_keeps_short_strings() {
        assert_eq!(short_sha("abc"), "abc");
        assert_eq!(short_sha("1.0.0"), "1.0.0");
    }

    #[test]
    fn short_sha_handles_non_hex_long_string() {
        assert_eq!(short_sha("abcdefgh1234567890"), "abcdefgh1234567890");
    }
}
