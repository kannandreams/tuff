use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const CORAL_GUIDE_CONTENT: &str = include_str!("../assets/coral-cli-guide.md");

use tabled::{
    settings::{object::Columns, Modify, Style, Width},
    Table, Tabled,
};

use crate::{
    adapter::{self, resolve_capability, AdapterKind},
    config, display,
    error::{CoralError, Result},
    git,
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
    #[tabled(rename = "AGENT")]
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
    #[tabled(rename = "AGENT")]
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

fn visible_width(text: &str) -> usize {
    let mut width = 0usize;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                let _ = chars.next();
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        width += 1;
    }

    width
}

fn pad_cell(text: &str, width: usize) -> String {
    let padding = width.saturating_sub(visible_width(text));
    format!("{text}{}", " ".repeat(padding))
}

fn border(left: char, join: char, right: char, widths: &[usize]) -> String {
    let mut line = String::new();
    line.push(left);
    for (index, width) in widths.iter().enumerate() {
        line.push_str(&"─".repeat(width + 2));
        if index + 1 == widths.len() {
            line.push(right);
        } else {
            line.push(join);
        }
    }
    line
}

fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|header| header.len()).collect();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(visible_width(cell));
        }
    }

    let mut out = String::new();
    out.push_str(&border('┌', '┬', '┐', &widths));
    out.push('\n');

    out.push('│');
    for (index, header) in headers.iter().enumerate() {
        out.push(' ');
        out.push_str(&pad_cell(header, widths[index]));
        out.push(' ');
        out.push('│');
    }
    out.push('\n');

    out.push_str(&border('├', '┼', '┤', &widths));
    out.push('\n');

    for row in rows {
        out.push('│');
        for (index, cell) in row.iter().enumerate() {
            out.push(' ');
            out.push_str(&pad_cell(cell, widths[index]));
            out.push(' ');
            out.push('│');
        }
        out.push('\n');
    }

    out.push_str(&border('└', '┴', '┘', &widths));
    out
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
        let mut config = config::read_config(repo_root)?;
        if !config.targets.iter().any(|target| target == "open-agents") {
            config.targets.push("open-agents".to_string());
            config::write_config(repo_root, &config)?;
        }

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
        let guide_path = repo_root
            .join(".agents")
            .join("skills")
            .join("coral-cli-guide");
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
                    ownership: lockfile::TargetOwnership::Generated,
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

pub fn cmd_create(repo_root: &Path, kind: &str, raw_id: &str, target_ids: &[String]) -> Result<()> {
    if !matches!(kind, "skill" | "tool" | "hook" | "workflow") {
        return Err(CoralError::new(format!("unknown capability type '{kind}'")));
    }
    let id = raw_id.trim();
    if id.is_empty() {
        return Err(CoralError::new("capability name must not be empty"));
    }

    let mut adapters = Vec::new();
    for target in target_ids {
        let adapter = AdapterKind::from_id(target).ok_or_else(|| {
            CoralError::new(format!(
                "unknown agent '{}'; use 'coral agent list' to see available agents",
                target
            ))
        })?;
        if !adapter.supports(kind) {
            return Err(CoralError::new(format!(
                "{} does not yet support {} capabilities",
                adapter.display_name(),
                kind
            )));
        }
        if !adapters.contains(&adapter) {
            adapters.push(adapter);
        }
    }

    let lock_path = lockfile::lockfile_path(repo_root);
    let mut coral_lock = if lock_path.exists() {
        lockfile::read_lockfile_at(&lock_path)?
    } else {
        lockfile::Lockfile {
            version: lockfile::LOCKFILE_VERSION,
            capabilities: BTreeMap::new(),
        }
    };
    if coral_lock.capabilities.contains_key(id) {
        return Err(CoralError::new(format!(
            "capability '{}' is already tracked; choose another id",
            id
        )));
    }

    let mut plans = Vec::new();
    for adapter in &adapters {
        let (relative_dir, files) = create_scaffold_files(kind, id, *adapter)?;
        let root = repo_root
            .join(adapter_project_dir(*adapter))
            .join(relative_dir)
            .join(id);
        if root.exists() {
            return Err(CoralError::new(format!(
                "refusing to overwrite existing capability scaffold: {}",
                lockfile::relative_or_absolute_fs(&root, repo_root)
            )));
        }
        plans.push((*adapter, root, files));
    }

    let mut config = config::read_config(repo_root)?;
    let mut target_entries = BTreeMap::new();
    let source_path = plans
        .first()
        .map(|(_, root, _)| lockfile::relative_or_absolute_fs(root, repo_root))
        .unwrap_or_default();

    for (adapter, root, files) in &plans {
        fs::create_dir_all(root)?;
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

        let baseline_dir = repo_root
            .join(".coral")
            .join("baselines")
            .join(adapter.id())
            .join(id);
        let mut emitted_files = Vec::new();
        for (name, content) in files {
            if *name == "coral.toml" {
                continue;
            }
            let target_path = root.join(name);
            let baseline_path = baseline_dir.join(name);
            if let Some(parent) = baseline_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&baseline_path, content)?;
            emitted_files.push(adapter::EmittedFile {
                path: lockfile::relative_or_absolute_fs(&target_path, repo_root),
                hash: lockfile::hash_bytes(content.as_bytes()),
            });
        }

        target_entries.insert(
            adapter.id().to_string(),
            TargetLockEntry {
                baseline_dir: lockfile::relative_or_absolute_fs(&baseline_dir, repo_root),
                emitted_files,
                ownership: lockfile::TargetOwnership::Generated,
            },
        );
        if !config.targets.iter().any(|target| target == adapter.id()) {
            config.targets.push(adapter.id().to_string());
        }

        for (name, _) in files {
            if *name != "coral.toml" {
                println!(
                    "created and tracked {} '{}' ({}) -> {}",
                    kind,
                    id,
                    adapter.id(),
                    lockfile::relative_or_absolute_fs(&root.join(name), repo_root)
                );
            }
        }

        if kind == "tool" {
            let mcp_path = match adapter {
                AdapterKind::OpenAgents => repo_root.join(".agents").join("mcp.json"),
                AdapterKind::Claude => repo_root.join(".mcp.json"),
            };
            let entrypoint = format!("{}/tools/{}/run.sh", adapter_project_dir(*adapter), id);
            crate::adapters::mcp_register_tool(repo_root, &mcp_path, id, "bash", &[entrypoint])?;
        }
    }

    coral_lock.capabilities.insert(
        id.to_string(),
        lockfile::CapabilityLockEntry {
            capability_type: kind.to_string(),
            installed_version: "0.1.0".to_string(),
            source_path,
            targets: target_entries,
            source: None,
            scope: "project".to_string(),
        },
    );
    config::write_config(repo_root, &config)?;
    lockfile::write_lockfile(repo_root, &coral_lock)?;
    println!("next: edit the generated files, then run `coral list` or `coral diff {id}`");
    Ok(())
}

fn adapter_project_dir(adapter: AdapterKind) -> &'static str {
    match adapter {
        AdapterKind::OpenAgents => ".agents",
        AdapterKind::Claude => ".claude",
    }
}

fn create_scaffold_files(
    kind: &str,
    id: &str,
    adapter: AdapterKind,
) -> Result<(&'static str, Vec<(&'static str, String)>)> {
    let files = match kind {
        "skill" => vec![
            (
                "coral.toml",
                format!(
                    "id = \"{id}\"\nversion = \"0.1.0\"\ntype = \"skill\"\ndescription = \"What this skill helps the agent do.\"\nfiles = [\"SKILL.md\"]\n"
                ),
            ),
            (
                "SKILL.md",
                format!(
                    "# {id}\n\n## Purpose\n\nDescribe when the agent should use this skill.\n\n## Guidance\n\n- Add the key rules the agent should follow.\n- Add examples, constraints, and team conventions.\n"
                ),
            ),
        ],
        "tool" => vec![
            (
                "coral.toml",
                format!(
                    "id = \"{id}\"\nversion = \"0.1.0\"\ntype = \"tool\"\ndescription = \"What this tool does for the agent.\"\nfiles = [\"run.sh\"]\n\n[parameters]\ntype = \"object\"\n\n[parameters.properties.input]\ntype = \"string\"\ndescription = \"Input passed to the tool\"\n\n[implementation]\nlanguage = \"bash\"\nentrypoint = \"run.sh\"\n"
                ),
            ),
            (
                "run.sh",
                "#!/usr/bin/env bash\nset -euo pipefail\n\necho \"replace with tool logic\"\n".to_string(),
            ),
        ],
        "hook" => {
            let file = match adapter {
                AdapterKind::OpenAgents => "hook.toml",
                AdapterKind::Claude => "hook.json",
            };
            let content = match adapter {
                AdapterKind::OpenAgents => {
                    "event = \"before_finish\"\ncommand = \"echo review hook\"\n".to_string()
                }
                AdapterKind::Claude => serde_json::to_string_pretty(&serde_json::json!({
                    "event": "before_finish",
                    "command": "echo review hook",
                    "working_directory": "."
                }))? + "\n",
            };
            vec![
                (
                    "coral.toml",
                    format!(
                        "id = \"{id}\"\nversion = \"0.1.0\"\ntype = \"hook\"\ndescription = \"What this hook enforces.\"\nfiles = [\"{file}\"]\n\n[hook]\nevent = \"before_finish\"\ncommand = \"echo review hook\"\nworking_directory = \".\"\n"
                    ),
                ),
                (file, content),
            ]
        }
        "workflow" => vec![
            (
                "coral.toml",
                format!(
                    "id = \"{id}\"\nversion = \"0.1.0\"\ntype = \"workflow\"\ndescription = \"When the agent should run this workflow.\"\nfiles = [\"workflow.toml\"]\n\n[[workflow.requires]]\nid = \"replace-me\"\ntype = \"skill\"\n"
                ),
            ),
            (
                "workflow.toml",
                format!(
                    "id = \"{id}\"\nversion = \"0.1.0\"\ntype = \"workflow\"\ndescription = \"When the agent should run this workflow.\"\n\n[[workflow.requires]]\nid = \"replace-me\"\ntype = \"skill\"\n"
                ),
            ),
        ],
        _ => unreachable!(),
    };
    let relative_dir = match kind {
        "skill" => "skills",
        "tool" => "tools",
        "hook" => "hooks",
        "workflow" => "workflows",
        _ => unreachable!(),
    };
    Ok((relative_dir, files))
}

#[allow(dead_code)]
fn cmd_create_legacy(
    repo_root: &Path,
    skill: Option<&str>,
    tool: Option<&str>,
    hook: Option<&str>,
    workflow: Option<&str>,
    target: &str,
) -> Result<()> {
    let adapter = AdapterKind::from_id(target).ok_or_else(|| {
        CoralError::new(format!(
            "unknown agent '{}'; use 'coral agent list' to see available agents",
            target
        ))
    })?;

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
            "choose exactly one scaffold type: --skill, --tool, --hook, or --workflow",
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
            vec![
                (
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
                ),
                (
                    "hook.toml",
                    "event = \"before_finish\"\ncommand = \"echo review hook\"\n".to_string(),
                ),
            ],
        ),
        "workflow" => (
            "workflows",
            vec![
                (
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
                ),
                (
                    "workflow.toml",
                    "name = \"replace-me\"\ndescription = \"Fill in the workflow steps here.\"\n"
                        .to_string(),
                ),
            ],
        ),
        _ => unreachable!(),
    };

    let target_root = match adapter {
        AdapterKind::OpenAgents => ".agents",
        AdapterKind::Claude => ".claude",
    };
    let canonical_target = adapter.id();
    let root = repo_root.join(target_root).join(relative_dir).join(id);
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
        "next: edit the generated files, then run `coral add {} -t {}`",
        lockfile::relative_or_absolute_fs(&root, repo_root),
        canonical_target
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

    install_capability(
        install_root,
        scope,
        &capability,
        &manifest,
        target_ids,
        Some(&SourceMetaInput {
            source_type: "git".to_string(),
            url: clean_url,
            source_ref: commit_sha.clone(),
            skill: name.to_string(),
        }),
    )
}

fn cmd_add_local(
    install_root: &Path,
    scope: Scope,
    capability: &Path,
    target_ids: &[String],
    project_root: &Path,
) -> Result<()> {
    let capability_dir = lockfile::absolutize(install_root, capability);
    let inferred = infer_from_path(&capability_dir);
    let manifest = load_or_synthetic_manifest(&capability_dir, Some(&inferred.0))?;
    let resolved = resolve_capability(&manifest)?;

    if scope == Scope::Project {
        if let Some(warning) = resolver::check_collision(&resolved.id, project_root, None)? {
            eprintln!("{warning}");
        }
    }

    if is_target_layout_path(install_root, &capability_dir) {
        return adopt_capability_in_place(
            install_root,
            scope,
            &capability_dir,
            &manifest,
            &resolved,
            &inferred.1,
            target_ids,
        );
    }

    install_capability(install_root, scope, &resolved, &manifest, target_ids, None)
}

fn load_or_synthetic_manifest(
    capability_dir: &Path,
    inferred_type: Option<&str>,
) -> Result<manifest::CapabilityManifest> {
    if capability_dir.join("coral.toml").exists() {
        load_manifest(capability_dir)
    } else {
        synthetic_local_manifest(capability_dir, inferred_type)
    }
}

fn synthetic_local_manifest(
    capability_dir: &Path,
    inferred_type: Option<&str>,
) -> Result<manifest::CapabilityManifest> {
    if !capability_dir.exists() || !capability_dir.is_dir() {
        return Err(CoralError::new(format!(
            "directory not found: {}",
            capability_dir.display()
        )));
    }

    let id = capability_dir
        .file_name()
        .ok_or_else(|| CoralError::new("capability directory must have a name"))?
        .to_string_lossy()
        .to_string();
    let capability_type = inferred_type.unwrap_or("skill").to_string();

    let mut files = Vec::new();
    for entry in fs::read_dir(capability_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name != "coral.toml" {
            files.push(name);
        }
    }
    files.sort();
    if files.is_empty() {
        return Err(CoralError::new(format!(
            "no source files found in {}",
            capability_dir.display()
        )));
    }

    Ok(manifest::CapabilityManifest {
        id,
        version: "0.1.0".into(),
        capability_type,
        description: "Added from existing agent assets.".into(),
        files,
        parameters: None,
        implementation: None,
        hook: None,
        workflow: None,
        targets: vec![],
        root: capability_dir.to_path_buf(),
    })
}

fn is_target_layout_path(root: &Path, capability_dir: &Path) -> bool {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let canonical_dir = capability_dir
        .canonicalize()
        .unwrap_or_else(|_| capability_dir.to_path_buf());
    let rel = canonical_dir
        .strip_prefix(&canonical_root)
        .unwrap_or(canonical_dir.as_path());
    matches!(
        rel.components()
            .next()
            .and_then(|component| component.as_os_str().to_str()),
        Some(".agents" | ".claude")
    )
}

fn relative_or_absolute_canonical(path: &Path, root: &Path) -> String {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical_path
        .strip_prefix(&canonical_root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| lockfile::relative_or_absolute_fs(path, root))
}

fn adopt_capability_in_place(
    install_root: &Path,
    scope: Scope,
    capability_dir: &Path,
    manifest: &manifest::CapabilityManifest,
    capability: &adapter::ResolvedCapability,
    inferred_target: &str,
    target_ids: &[String],
) -> Result<()> {
    for target_id in target_ids {
        if target_id != inferred_target {
            return Err(CoralError::new(format!(
                "{} is already in the '{}' agent layout; use -a {}",
                lockfile::relative_or_absolute_fs(capability_dir, install_root),
                inferred_target,
                inferred_target
            )));
        }
    }

    if !capability_dir.join("coral.toml").exists() {
        let toml_content = format!(
            "# Generated by coral add\nid = \"{}\"\nversion = \"{}\"\ntype = \"{}\"\ndescription = \"Added from existing agent assets.\"\nfiles = [{}]\n",
            capability.id,
            capability.version,
            capability.capability_type,
            manifest
                .files
                .iter()
                .map(|file| format!("\"{file}\""))
                .collect::<Vec<_>>()
                .join(", ")
        );
        fs::write(capability_dir.join("coral.toml"), toml_content)?;
    }

    let mut lockfile = lockfile::require_lockfile(install_root)?;
    if lockfile.capabilities.contains_key(&capability.id) {
        return Err(CoralError::new(format!(
            "capability '{}' is already tracked; use 'coral update {}' for tracked changes",
            capability.id, capability.id
        )));
    }

    let baseline_dir = install_root
        .join(".coral")
        .join("baselines")
        .join(inferred_target)
        .join(&capability.id);
    fs::create_dir_all(&baseline_dir)?;

    let mut emitted_files = Vec::new();
    for (rel_path, content) in &capability.source_files {
        let file_path = capability_dir.join(rel_path);
        let baseline_path = baseline_dir.join(rel_path);
        if let Some(parent) = baseline_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&baseline_path, content)?;
        emitted_files.push(adapter::EmittedFile {
            path: relative_or_absolute_canonical(&file_path, install_root),
            hash: lockfile::hash_bytes(&fs::read(&file_path)?),
        });
    }

    let mut targets = BTreeMap::new();
    targets.insert(
        inferred_target.to_string(),
        lockfile::TargetLockEntry {
            baseline_dir: lockfile::relative_or_absolute_fs(&baseline_dir, install_root),
            emitted_files,
            ownership: lockfile::TargetOwnership::Imported,
        },
    );

    lockfile.capabilities.insert(
        capability.id.clone(),
        lockfile::CapabilityLockEntry {
            capability_type: capability.capability_type.clone(),
            installed_version: capability.version.clone(),
            source_path: relative_or_absolute_canonical(capability_dir, install_root),
            targets,
            source: None,
            scope: scope.as_str().to_string(),
        },
    );
    lockfile::write_lockfile(install_root, &lockfile)?;
    println!(
        "added {} ({}, {}) -> {}",
        capability.id,
        capability.capability_type,
        inferred_target,
        relative_or_absolute_canonical(capability_dir, install_root)
    );
    Ok(())
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
                "unknown agent '{}'; run 'coral agent list' to see available agents",
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
                if !adapter
                    .supported_events()
                    .contains(&hook_cfg.event.as_str())
                {
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
                ownership: lockfile::TargetOwnership::Generated,
            },
        );
    }

    // MCP registration for tool primitives
    if capability.capability_type == "tool" {
        if let Some(ref impl_cfg) = capability.implementation {
            for adapter in &adapters {
                let mcp_path = match adapter {
                    AdapterKind::OpenAgents => install_root.join(".agents").join("mcp.json"),
                    AdapterKind::Claude => install_root.join(".mcp.json"),
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

    let table_rows: Vec<Vec<String>> = rows
        .into_iter()
        .map(|row| {
            vec![
                row.id,
                row.capability_type,
                row.version,
                row.scope,
                row.target,
                row.status,
                row.path,
            ]
        })
        .collect();
    println!(
        "{}",
        render_table(
            &["ID", "TYPE", "VERSION", "SCOPE", "AGENT", "STATUS", "PATH"],
            &table_rows
        )
    );
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
                            if line.starts_with("id = \"")
                                && !line.contains("version")
                                && !line.contains("description")
                                && !line.contains("type")
                            {
                                current_id = line
                                    .trim_start_matches("id = \"")
                                    .trim_end_matches('"')
                                    .to_string();
                            }
                            if line.starts_with("type = \"") {
                                let ctype = line
                                    .trim_start_matches("type = \"")
                                    .trim_end_matches('"')
                                    .to_string();
                                if !current_id.is_empty() && ctype != "workflow" {
                                    requires.push((current_id.clone(), ctype));
                                    current_id.clear();
                                }
                            }
                        }

                        for (req_id, req_type) in &requires {
                            let child_status = if let Ok(lf) = lockfile::require_lockfile(repo_root)
                            {
                                if let Some(child_entry) = lf.capabilities.get(req_id) {
                                    let mut child_drift = Vec::new();
                                    for (_, ct_entry) in &child_entry.targets {
                                        for e in &ct_entry.emitted_files {
                                            let s = lockfile::drift_status(repo_root, e);
                                            if s != "clean" {
                                                child_drift.push(s);
                                            }
                                        }
                                    }
                                    if child_drift.is_empty() {
                                        "clean".to_string()
                                    } else {
                                        child_drift.join(",")
                                    }
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

    print!("{}", style_diff(&output));
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

            let upstream_content =
                crate::diff::get_upstream_content(&cache_dir, &source.skill, &file_name)?;
            let baseline_path = baseline_dir.join(file_name.as_ref());
            let baseline_content = std::fs::read_to_string(&baseline_path)?;

            if baseline_content == upstream_content {
                continue;
            }

            output.push_str(&format!(
                "--- baseline/{}\n+++ upstream/{}/{}\n",
                file_name, tid, file_name
            ));
            let diff = similar::TextDiff::from_lines(&baseline_content, &upstream_content);
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
        print!("{}", style_diff(&output));
    }

    Ok(())
}

fn style_diff(diff: &str) -> String {
    diff.split_inclusive('\n')
        .map(|line| {
            let code = if line.starts_with("+++") || line.starts_with("---") {
                "36"
            } else if line.starts_with('+') {
                "32"
            } else if line.starts_with('-') {
                "31"
            } else {
                return line.to_string();
            };
            paint(line, code)
        })
        .collect()
}

// ── Delete / untrack ────────────────────────────────────────────────────────

fn resolve_cleanup_scope(repo_root: &Path, scope_str: &str) -> Result<(Scope, PathBuf)> {
    let scope = resolver::Scope::from_str(scope_str)
        .ok_or_else(|| CoralError::new(format!("invalid scope '{}'", scope_str)))?;
    let scope_root = match scope {
        Scope::Project => repo_root.to_path_buf(),
        Scope::Global => home_dir()?,
    };
    Ok((scope, scope_root))
}

fn canonical_cleanup_targets(targets: &[String]) -> Result<Vec<String>> {
    let mut canonical = BTreeSet::new();
    for target in targets {
        let adapter = AdapterKind::from_id(target).ok_or_else(|| {
            CoralError::new(format!(
                "unknown agent '{}'; use 'coral agent list' to see available agents",
                target
            ))
        })?;
        canonical.insert(adapter.id().to_string());
    }
    Ok(canonical.into_iter().collect())
}

fn remove_target_tracking(
    scope_root: &Path,
    id: &str,
    entry: &mut lockfile::CapabilityLockEntry,
    target: &str,
) -> Result<()> {
    if let Some(target_entry) = entry.targets.remove(target) {
        let baseline_dir = scope_root.join(&target_entry.baseline_dir);
        if baseline_dir.exists() {
            std::fs::remove_dir_all(baseline_dir)?;
        }
    } else {
        return Err(CoralError::new(format!(
            "'{}' is not tracked for agent '{}'",
            id, target
        )));
    }
    Ok(())
}

pub fn cmd_delete(
    repo_root: &Path,
    id: &str,
    scope_str: &str,
    targets: &[String],
    force: bool,
) -> Result<()> {
    let (scope, scope_root) = resolve_cleanup_scope(repo_root, scope_str)?;
    let target_ids = canonical_cleanup_targets(targets)?;

    let mut lockfile = lockfile::require_lockfile(&scope_root)?;
    let mut entry = lockfile.capabilities.get(id).cloned().ok_or_else(|| {
        CoralError::new(format!(
            "'{}' is not installed in {} scope",
            id,
            scope.as_str()
        ))
    })?;

    for target in &target_ids {
        let target_entry = entry.targets.get(target).ok_or_else(|| {
            CoralError::new(format!("'{}' is not tracked for agent '{}'", id, target))
        })?;

        if target_entry.ownership == lockfile::TargetOwnership::Imported {
            return Err(CoralError::new(format!(
                "'{}' is tracked in place for agent '{}'; use 'coral untrack {} -a {}' instead",
                id, target, id, target
            )));
        }

        let modified = target_entry
            .emitted_files
            .iter()
            .any(|emitted| lockfile::drift_status(&scope_root, emitted) == "modified");
        if modified && !force {
            return Err(CoralError::new(format!(
                "'{}' has local modifications for agent '{}'; use --force to delete",
                id, target
            )));
        }
        if modified {
            eprintln!(
                "warning: '{}' has local modifications for agent '{}' - deleting them",
                id, target
            );
        }
    }

    for target in &target_ids {
        if let Some(adapter) = AdapterKind::from_id(target) {
            adapter.remove(id, &scope_root)?;
        }
        remove_target_tracking(&scope_root, id, &mut entry, target)?;
    }

    if entry.targets.is_empty() {
        lockfile.capabilities.remove(id);
    } else {
        lockfile.capabilities.insert(id.to_string(), entry);
    }
    lockfile::write_lockfile(&scope_root, &lockfile)?;
    println!("deleted '{}' from {} scope", id, scope.as_str());
    Ok(())
}

pub fn cmd_untrack(repo_root: &Path, id: &str, scope_str: &str, targets: &[String]) -> Result<()> {
    let (scope, scope_root) = resolve_cleanup_scope(repo_root, scope_str)?;
    let target_ids = canonical_cleanup_targets(targets)?;

    let mut lockfile = lockfile::require_lockfile(&scope_root)?;
    let mut entry = lockfile.capabilities.get(id).cloned().ok_or_else(|| {
        CoralError::new(format!(
            "'{}' is not installed in {} scope",
            id,
            scope.as_str()
        ))
    })?;

    for target in &target_ids {
        if !entry.targets.contains_key(target) {
            return Err(CoralError::new(format!(
                "'{}' is not tracked for agent '{}'",
                id, target
            )));
        }
    }

    for target in &target_ids {
        remove_target_tracking(&scope_root, id, &mut entry, target)?;
    }

    if entry.targets.is_empty() {
        lockfile.capabilities.remove(id);
    } else {
        lockfile.capabilities.insert(id.to_string(), entry);
    }
    lockfile::write_lockfile(&scope_root, &lockfile)?;
    println!("untracked '{}' from {} scope", id, scope.as_str());
    Ok(())
}

// ── Update ──────────────────────────────────────────────────────────────────

fn select_update_targets(
    id: &str,
    entry: &lockfile::CapabilityLockEntry,
    requested: &[String],
) -> Result<Vec<String>> {
    if requested.is_empty() {
        return Ok(entry.targets.keys().cloned().collect());
    }

    for target_id in requested {
        if !entry.targets.contains_key(target_id) {
            return Err(CoralError::new(format!(
                "'{}' is not installed for agent '{}'",
                id, target_id
            )));
        }
    }

    Ok(requested.to_vec())
}

fn update_local_baseline(
    scope_root: &Path,
    id: &str,
    target_ids: &[String],
    check: bool,
    force: bool,
) -> Result<()> {
    if force {
        return Err(CoralError::new(
            "--force is only valid for git-sourced capabilities; local updates accept the current files as the new baseline",
        ));
    }

    let lockfile = lockfile::require_lockfile(scope_root)?;
    let entry = lockfile
        .capabilities
        .get(id)
        .ok_or_else(|| CoralError::new(format!("'{}' is not installed", id)))?;

    let mut updates = Vec::new();
    let mut changed_files = 0usize;

    // Preflight every selected file before writing any baseline or lockfile data.
    for target_id in target_ids {
        let target_entry = entry.targets.get(target_id).ok_or_else(|| {
            CoralError::new(format!(
                "'{}' is not installed for agent '{}'",
                id, target_id
            ))
        })?;

        for emitted in &target_entry.emitted_files {
            let local_path = scope_root.join(&emitted.path);
            if !local_path.is_file() {
                return Err(CoralError::new(format!(
                    "tracked file is missing for '{}': {}",
                    id,
                    local_path.display()
                )));
            }

            let file_name = Path::new(&emitted.path).file_name().ok_or_else(|| {
                CoralError::new(format!("invalid emitted file path: {}", emitted.path))
            })?;
            let baseline_path = scope_root.join(&target_entry.baseline_dir).join(file_name);
            if !baseline_path.is_file() {
                return Err(CoralError::new(format!(
                    "baseline file is missing for '{}': {}",
                    id,
                    baseline_path.display()
                )));
            }

            let content = fs::read(&local_path)?;
            let baseline = fs::read(&baseline_path)?;
            if content != baseline {
                changed_files += 1;
            }
            updates.push((
                target_id.clone(),
                emitted.path.clone(),
                baseline_path,
                content,
            ));
        }
    }

    if check {
        if changed_files == 0 {
            println!("'{}' is already up to date", id);
        } else {
            println!(
                "'{}' has {} local file(s) — update would record them as the new baseline",
                id, changed_files
            );
        }
        return Ok(());
    }

    for (_target_id, _path, baseline_path, content) in &updates {
        if let Some(parent) = baseline_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(baseline_path, content)?;
    }

    let mut lockfile = lockfile;
    let installed = lockfile
        .capabilities
        .get_mut(id)
        .ok_or_else(|| CoralError::new(format!("'{}' is not installed", id)))?;
    for (target_id, path, _baseline_path, content) in updates {
        let target_entry = installed.targets.get_mut(&target_id).ok_or_else(|| {
            CoralError::new(format!(
                "'{}' is not installed for agent '{}'",
                id, target_id
            ))
        })?;
        let emitted = target_entry
            .emitted_files
            .iter_mut()
            .find(|emitted| emitted.path == path)
            .ok_or_else(|| CoralError::new(format!("tracked file disappeared: {}", path)))?;
        emitted.hash = lockfile::hash_bytes(&content);
    }
    lockfile::write_lockfile(scope_root, &lockfile)?;

    if changed_files == 0 {
        println!("'{}' is already up to date", id);
    } else {
        println!(
            "updated local baseline for '{}' ({} file(s))",
            id, changed_files
        );
    }
    Ok(())
}

fn is_in_place_source_path(source_path: &str) -> bool {
    source_path.is_empty()
        || source_path.starts_with(".agents/")
        || source_path == ".agents"
        || source_path.starts_with(".claude/")
        || source_path == ".claude"
}

fn update_local_from_source(
    scope_root: &Path,
    scope: Scope,
    id: &str,
    entry: &lockfile::CapabilityLockEntry,
    target_ids: &[String],
    check: bool,
    force: bool,
) -> Result<()> {
    let source_dir = lockfile::absolutize(scope_root, Path::new(&entry.source_path));
    let manifest = load_manifest(&source_dir)?;
    let capability = resolve_capability(&manifest)?;
    if capability.id != id {
        return Err(CoralError::new(format!(
            "sourcePath for '{}' points at capability '{}'",
            id, capability.id
        )));
    }

    let mut adapters = Vec::new();
    for tid in target_ids {
        let adapter = AdapterKind::from_id(tid).ok_or_else(|| {
            CoralError::new(format!(
                "unknown agent '{}'; run 'coral agent list' to see available agents",
                tid
            ))
        })?;
        adapters.push(adapter);
    }

    let mut changed_files = 0usize;
    let mut has_local_drift = false;
    for adapter in &adapters {
        let planned_files = adapter.plan(&capability, scope_root)?;
        let target_entry = entry.targets.get(adapter.id()).ok_or_else(|| {
            CoralError::new(format!(
                "'{}' is not installed for agent '{}'",
                id,
                adapter.id()
            ))
        })?;
        for planned in &planned_files {
            let current_path = scope_root.join(&planned.path);
            let current = if current_path.exists() {
                fs::read(&current_path)?
            } else {
                Vec::new()
            };
            if current != planned.content {
                changed_files += 1;
            }
        }
        has_local_drift |= target_entry
            .emitted_files
            .iter()
            .any(|emitted| lockfile::drift_status(scope_root, emitted) != "clean");
    }

    if check {
        if has_local_drift && !force {
            println!(
                "'{}' has local changes — update would require --force to reload from local source",
                id
            );
        } else if changed_files == 0 {
            println!("'{}' is already up to date", id);
        } else {
            println!(
                "'{}' has {} source file(s) to apply from {}",
                id, changed_files, entry.source_path
            );
        }
        return Ok(());
    }

    if has_local_drift && !force {
        return Err(CoralError::new(format!(
            "'{}' has local changes; run 'coral diff {}' first or use --force to reload from source",
            id, id
        )));
    }

    install_capability(scope_root, scope, &capability, &manifest, target_ids, None)?;
    Ok(())
}

pub fn cmd_update(
    repo_root: &Path,
    id: &str,
    scope_str: Option<&str>,
    requested_targets: &[String],
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

    let target_ids = select_update_targets(id, &entry, requested_targets)?;

    if entry.source.is_none() && is_in_place_source_path(&entry.source_path) {
        return update_local_baseline(&scope_root, id, &target_ids, check, force);
    }

    if entry.source.is_none() {
        return update_local_from_source(&scope_root, scope, id, &entry, &target_ids, check, force);
    }

    let source = entry.source.as_ref().expect("source checked above");

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

    for target_id in &target_ids {
        let target_entry = entry.targets.get(target_id).ok_or_else(|| {
            CoralError::new(format!(
                "'{}' is not installed for agent '{}'",
                id, target_id
            ))
        })?;
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
                let report =
                    crate::diff::merge_and_write(&baseline_path, &local_path, &upstream_path)?;

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
            println!(
                "'{}' has local changes — update would attempt three-way merge",
                id
            );
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

fn infer_from_path(path: &Path) -> (String, String) {
    let parent = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let ctype = match parent.as_str() {
        "tools" => "tool",
        "hooks" => "hook",
        "workflows" => "workflow",
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

    let table_rows: Vec<Vec<String>> = styled_rows
        .into_iter()
        .map(|row| {
            vec![
                row.id,
                row.capability_type,
                row.target,
                row.current,
                row.latest,
                row.status,
            ]
        })
        .collect();

    println!(
        "{}",
        render_table(
            &["ID", "TYPE", "AGENT", "CURRENT", "LATEST", "STATUS"],
            &table_rows
        )
    );
    Ok(())
}

// ── Agent commands ──────────────────────────────────────────────────────────

pub fn cmd_agent_list(repo_root: &Path) -> Result<()> {
    #[derive(Tabled)]
    struct AgentRow {
        #[tabled(rename = "AGENT")]
        agent: String,
        #[tabled(rename = "AGENTS SUPPORTED")]
        agents: String,
        #[tabled(rename = "PRIMITIVES")]
        primitives: String,
    }

    let config = config::read_config(repo_root)?;
    let registered: std::collections::HashSet<&str> =
        config.targets.iter().map(|s| s.as_str()).collect();

    let rows: Vec<AgentRow> = AdapterKind::all()
        .iter()
        .map(|a| {
            let agent_label = if registered.contains(a.id()) {
                format!("{} *", a.id())
            } else {
                a.id().to_string()
            };
            AgentRow {
                agent: agent_label,
                agents: a.supported_agents().join(", "),
                primitives: a.kinds_supported().join(", "),
            }
        })
        .collect();

    let mut table = Table::new(rows);
    table
        .with(Style::modern())
        .with(Modify::new(Columns::single(1)).with(Width::wrap(40).keep_words(true)));

    println!("{table}");

    println!(
        "\n  {} = registered (use 'coral agent add <id>' to register)",
        paint("*", "32")
    );
    Ok(())
}

pub fn cmd_agent_add(repo_root: &Path, id: &str) -> Result<()> {
    let adapter = AdapterKind::from_id(id).ok_or_else(|| {
        CoralError::new(format!(
            "unknown agent '{}'; use 'coral agent list' to see available agents",
            id
        ))
    })?;

    let mut config = config::read_config(repo_root)?;
    adapter.ensure_project_dir(repo_root)?;
    if config.targets.contains(&adapter.id().to_string()) {
        println!("agent '{}' is already registered", id);
        return Ok(());
    }

    config.targets.push(adapter.id().to_string());
    config::write_config(repo_root, &config)?;
    println!(
        "registered agent '{}' ({})",
        adapter.id(),
        adapter.display_name()
    );
    Ok(())
}

pub fn cmd_agent_remove(repo_root: &Path, id: &str) -> Result<()> {
    let adapter = AdapterKind::from_id(id).ok_or_else(|| {
        CoralError::new(format!(
            "unknown agent '{}'; use 'coral agent list' to see available agents",
            id
        ))
    })?;

    let mut config = config::read_config(repo_root)?;
    let was_registered = config.targets.contains(&adapter.id().to_string());

    config.targets.retain(|t| t != adapter.id());
    config::write_config(repo_root, &config)?;

    if was_registered {
        println!("unregistered agent '{}'", adapter.id());
    } else {
        println!("removed agent '{}'", adapter.id());
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
