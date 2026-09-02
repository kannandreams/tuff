use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::adapter::{AdapterKind, AgentAdapter};
use crate::config;
use crate::error::{Result, TuffError};
use crate::lockfile::{self, TargetLockEntry};
use crate::manifest::CapabilityType;

use super::resolve_agent_selection;

pub fn cmd_create(
    repo_root: &Path,
    kind: CapabilityType,
    raw_id: &str,
    target_ids: &[String],
) -> Result<()> {
    let id = raw_id.trim();
    if id.is_empty() {
        return Err(TuffError::usage("capability name must not be empty"));
    }

    let target_ids = resolve_agent_selection(repo_root, target_ids, false)?;
    let mut adapters = Vec::new();
    for target in &target_ids {
        let adapter = AdapterKind::from_id(target).ok_or_else(|| {
            TuffError::usage(format!("unknown agent '{}'", target,))
                .with_hint("run 'tuff agent list' to see available agents")
        })?;
        if !adapter.supports(kind) {
            return Err(TuffError::unsupported(format!(
                "{} does not yet support {} capabilities",
                adapter.display_name(),
                kind
            )));
        }
        if !adapters.contains(&adapter) {
            adapters.push(adapter);
        }
    }

    let lock_path = lockfile::project_lockfile(repo_root);
    let mut tuff_lock = if lock_path.exists() {
        lockfile::read_lockfile_at(&lock_path)?
    } else {
        lockfile::Lockfile {
            version: lockfile::LOCKFILE_VERSION,
            capabilities: BTreeMap::new(),
        }
    };
    if tuff_lock.capabilities.contains_key(id) {
        return Err(
            TuffError::refused(format!("capability '{}' is already tracked", id))
                .with_hint("choose another id"),
        );
    }

    let mut plans = Vec::new();
    for adapter in &adapters {
        let (relative_dir, files) = create_scaffold_files(kind, id, *adapter)?;
        let root = repo_root
            .join(adapter.dir_prefix())
            .join(relative_dir)
            .join(id);
        if root.exists() {
            return Err(TuffError::refused(format!(
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
        if kind == CapabilityType::Tool || kind == CapabilityType::Hook {
            use std::os::unix::fs::PermissionsExt;
            let run_sh = root.join("run.sh");
            let mut permissions = fs::metadata(&run_sh)?.permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(run_sh, permissions)?;
        }

        let baseline_hash = crate::cache::hash_tree(root)?;
        crate::cache::populate(&super::home_dir()?, &baseline_hash, root)?;

        let mut managed_hooks = Vec::new();

        if kind == CapabilityType::Hook {
            let settings_relpath = adapter.hook_settings_relpath();
            let settings_path = repo_root.join(settings_relpath);
            let event = adapter.scaffold_hook_event();
            let fragment = adapter.command_hook_fragment(
                event,
                &format!("sh {}/hooks/{id}/run.sh", adapter.dir_prefix()),
            );
            let existing = if settings_path.is_file() {
                Some(fs::read(&settings_path)?)
            } else {
                None
            };
            let merged = adapter.merge_hook_fragment(existing.as_deref(), &fragment)?;
            if let Some(parent) = settings_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&settings_path, &merged)?;
            let canonical_event = adapter.canonical_hook_event(event)?;
            managed_hooks = lockfile::managed_hooks_from_fragment_with_canonical(
                repo_root,
                settings_relpath,
                &fragment,
                Some(canonical_event),
            )?;
            println!(
                "created and tracked hook '{}' ({}) -> {}",
                id,
                adapter.id(),
                lockfile::relative_or_absolute_fs(&settings_path, repo_root)
            );
        }

        target_entries.insert(
            adapter.id().to_string(),
            TargetLockEntry {
                managed_hooks,
                managed_mcp_entry: None,
                ownership: lockfile::TargetOwnership::Generated,
                sha256: baseline_hash,
                installed_path: lockfile::relative_or_absolute_fs(root, repo_root),
            },
        );
        if !config.agents.iter().any(|agent| agent == adapter.id()) {
            config.agents.push(adapter.id().to_string());
        }

        for (name, _) in files {
            println!(
                "created and tracked {} '{}' ({}) -> {}",
                kind,
                id,
                adapter.id(),
                lockfile::relative_or_absolute_fs(&root.join(name), repo_root)
            );
        }
    }

    tuff_lock.capabilities.insert(
        id.to_string(),
        lockfile::CapabilityLockEntry {
            capability_type: kind,
            version: "0.1.0".to_string(),
            version_scheme: lockfile::VersionScheme::Declared,
            description: default_capability_description(kind).to_string(),
            source: lockfile::CapabilitySource::local(source_path),
            targets: target_entries,
            implementation: None,
            parameters: None,
            workflow: None,
            server: None,
        },
    );
    config::write_config(repo_root, &config)?;
    lockfile::write_lockfile(repo_root, &tuff_lock)?;
    println!("next: edit the generated files, then run `tuff list` or `tuff diff {id}`");
    Ok(())
}

fn default_capability_description(kind: CapabilityType) -> &'static str {
    match kind {
        CapabilityType::Skill => "What this skill helps the agent do.",
        CapabilityType::Tool => "What this tool does for the agent.",
        CapabilityType::Hook => "What this hook enforces.",
        CapabilityType::Workflow => "When the agent should run this workflow.",
        CapabilityType::Policy => "Policy capabilities are not scaffolded yet.",
        CapabilityType::McpServer => "MCP servers are added, not scaffolded.",
    }
}

fn create_scaffold_files(
    kind: CapabilityType,
    id: &str,
    adapter: AdapterKind,
) -> Result<(&'static str, Vec<(&'static str, String)>)> {
    let files = match kind {
        CapabilityType::Skill => vec![(
            "SKILL.md",
            format!(
                "# {id}\n\n## Purpose\n\nDescribe when the agent should use this skill.\n\n## Guidance\n\n- Add the key rules the agent should follow.\n- Add examples, constraints, and team conventions.\n"
            ),
        )],
        CapabilityType::Tool => vec![(
            "run.sh",
            "#!/usr/bin/env bash\nset -euo pipefail\n\necho \"replace with tool logic\"\n"
                .to_string(),
        )],
        CapabilityType::Hook => {
            if adapter == AdapterKind::Claude || adapter == AdapterKind::OpenAgents {
                return Ok((
                    kind.plural_dir(),
                    vec![
                        (
                            "run.sh",
                            "#!/usr/bin/env bash\nset -euo pipefail\n\necho \"replace with hook logic\"\n"
                                .to_string(),
                        ),
                    ],
                ));
            }
            let file = adapter.hook_filename();
            let content = adapter
                .hook_file_content(&crate::manifest::HookConfig {
                    event: "before_finish".into(),
                    command: "echo review hook".into(),
                    working_directory: ".".into(),
                })
                .and_then(|bytes| {
                    String::from_utf8(bytes).map_err(|e| {
                        TuffError::corrupt(format!("hook content is not valid UTF-8: {e}"))
                    })
                })?;
            vec![(file, content)]
        }
        CapabilityType::Workflow => vec![(
            "workflow.toml",
            format!(
                "id = \"{id}\"\nversion = \"0.1.0\"\ntype = \"workflow\"\ndescription = \"When the agent should run this workflow.\"\n\n[[workflow.requires]]\nid = \"replace-me\"\ntype = \"skill\"\n"
            ),
        )],
        CapabilityType::Policy => {
            return Err(TuffError::unsupported(
                "policy capabilities are not scaffolded yet",
            ));
        }
        CapabilityType::McpServer => {
            return Err(TuffError::unsupported(
                "mcp-server capabilities are not scaffolded; use `tuff add mcp <catalog-id|path|git-url>` instead",
            ));
        }
    };
    let relative_dir = kind.plural_dir();
    Ok((relative_dir, files))
}
