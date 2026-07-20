use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::adapter::{self, AdapterKind, CapabilityKind, resolve_capability, AgentAdapter};
use crate::error::{CoralError, Result};
use crate::git;
use crate::lockfile::{self, TargetLockEntry};
use crate::manifest::{self, load_manifest, CapabilityType};
use crate::resolver::{self, Scope};

use super::{home_dir, infer_from_path, resolve_agent_selection};

pub fn cmd_add(
    repo_root: &Path,
    source: Option<&Path>,
    name: Option<&str>,
    capability_type: Option<&str>,
    target_ids: &[String],
    global: bool,
) -> Result<()> {
    let source = source.ok_or_else(|| CoralError::new("source path or URL is required"))?;
    let (scope, install_root) = if global {
        let home = home_dir()?;
        let lock_path = home.join(".coral").join("coral-lock.json");
        lockfile::init_lockfile_at(&lock_path)?;
        (Scope::Global, home)
    } else {
        (Scope::Project, repo_root.to_path_buf())
    };
    let target_ids = resolve_agent_selection(&install_root, target_ids)?;

    if git::is_git_url(&source.to_string_lossy()) {
        return cmd_add_git(
            &install_root,
            scope,
            &source.to_string_lossy(),
            &target_ids,
            name,
            capability_type,
            repo_root,
        );
    }
    cmd_add_local(&install_root, scope, source, &target_ids, repo_root, capability_type, name)
}

fn cmd_add_git(
    install_root: &Path,
    scope: Scope,
    url: &str,
    target_ids: &[String],
    name: Option<&str>,
    capability_type: Option<&str>,
    project_root: &Path,
) -> Result<()> {
    let name = name.ok_or_else(|| {
        CoralError::new("--name is required when installing from a git URL")
    })?;

    let (cache_dir, clean_url) = git::clone_or_fetch(url)?;
    let commit_sha = git::resolve_ref(&cache_dir)?;
    let cap_type = capability_type
        .and_then(CapabilityType::from_str)
        .unwrap_or(CapabilityType::Skill);
    let skill_dir = git::discover_capability(&cache_dir, name, cap_type)?;

    let manifest = manifest::synthetic_manifest(&skill_dir, name, &commit_sha)?;
    let capability = resolve_capability(&manifest)?;

    if scope == Scope::Project
        && let Some(warning) = resolver::check_collision(name, project_root, Some(&clean_url))? {
            eprintln!("{warning}");
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
            source_ref: commit_sha,
            skill: name.to_string(),
        }),
    )
}

fn cmd_add_local(
    install_root: &Path,
    scope: Scope,
    capability_path: &Path,
    target_ids: &[String],
    project_root: &Path,
    capability_type: Option<&str>,
    _name: Option<&str>,
) -> Result<()> {
    let capability_dir = lockfile::absolutize(install_root, capability_path);
    let parsed_type = capability_type.and_then(CapabilityType::from_str);
    let inferred = infer_from_path(&capability_dir);
    let resolved_type = parsed_type.or(Some(inferred.0));
    let manifest = load_or_synthetic_manifest(&capability_dir, resolved_type)?;
    let resolved = resolve_capability(&manifest)?;

    if scope == Scope::Project
        && let Some(warning) = resolver::check_collision(&resolved.id, project_root, None)? {
            eprintln!("{warning}");
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
    inferred_type: Option<CapabilityType>,
) -> Result<manifest::CapabilityManifest> {
    if capability_dir.join("coral.toml").exists() {
        load_manifest(capability_dir)
    } else {
        synthetic_local_manifest(capability_dir, inferred_type)
    }
}

fn synthetic_local_manifest(
    capability_dir: &Path,
    inferred_type: Option<CapabilityType>,
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
    let capability_type = inferred_type.unwrap_or(CapabilityType::Skill);

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
    _manifest: &manifest::CapabilityManifest,
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

    let mut lockfile = lockfile::require_lockfile(install_root)?;
    if lockfile.capabilities.contains_key(&capability.id) {
        return Err(CoralError::new(format!(
            "capability '{}' is already tracked; use 'coral update {}' for tracked changes",
            capability.id, capability.id
        )));
    }

    let mut emitted_files = Vec::new();
    for (rel_path, content) in &capability.source_files {
        let file_path = capability_dir.join(rel_path);
        emitted_files.push(adapter::EmittedFile {
            path: relative_or_absolute_canonical(&file_path, install_root),
            hash: lockfile::hash_bytes(&fs::read(&file_path)?),
            baseline_hash: lockfile::write_baseline_object(install_root, content)?,
        });
    }

    let mut targets = BTreeMap::new();
    targets.insert(
        inferred_target.to_string(),
        lockfile::TargetLockEntry {
            emitted_files,
            ownership: lockfile::TargetOwnership::Imported,
        },
    );

    lockfile.capabilities.insert(
        capability.id.clone(),
        lockfile::CapabilityLockEntry {
            capability_type: capability.capability_type,
            installed_version: capability.version.clone(),
            description: capability.description.clone(),
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

pub(crate) struct SourceMetaInput {
    pub source_type: String,
    pub url: String,
    pub source_ref: String,
    pub skill: String,
}

pub(crate) fn install_capability(
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
        if !adapter.supports(capability.capability_type) {
            return Err(CoralError::new(format!(
                "{} does not yet support {} capabilities",
                adapter.display_name(),
                capability.capability_type
            )));
        }
        if let CapabilityKind::Hook { hook: ref hook_cfg } = capability.kind
            && !adapter
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
        adapters.push(adapter);
    }

    let mut plans: Vec<(AdapterKind, Vec<adapter::PlannedFile>)> = Vec::new();
    for adapter in &adapters {
        let planned = adapter.plan(capability, install_root)?;
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
                baseline_hash: lockfile::write_baseline_object(install_root, &planned.content)?,
            });

            if should_print_installed_file(capability, planned) {
                println!(
                    "installed {} ({}) -> {}",
                    capability.id,
                    adapter.id(),
                    lockfile::relative_or_absolute_fs(&target_path, install_root)
                );
            }
        }

        new_targets.insert(
            adapter.id().to_string(),
            TargetLockEntry {
                emitted_files: emitted,
                ownership: lockfile::TargetOwnership::Generated,
            },
        );
    }

    if let CapabilityKind::Tool { implementation: ref impl_cfg, .. } = capability.kind {
        if impl_cfg.mcp {
                for adapter in &adapters {
                    let mcp_path = install_root.join(adapter.mcp_config_relpath());

                    let mcp_command = impl_cfg.language.clone();
                    let entrypoint_path = format!(
                        "{}/tools/{}/{}",
                        adapter.dir_prefix(),
                        capability.id,
                        impl_cfg.entrypoint
                    );
                    let mcp_args = vec![entrypoint_path];

                    crate::adapters::mcp_register_tool(
                        install_root,
                        &mcp_path,
                        &capability.id,
                        &mcp_command,
                        &mcp_args,
                    )?;
                    println!(
                        "registered MCP server {} ({}) -> {}",
                        capability.id,
                        adapter.id(),
                        lockfile::relative_or_absolute_fs(&mcp_path, install_root)
                    );
                }
            } else {
                for adapter in &adapters {
                    let mcp_path = install_root.join(adapter.mcp_config_relpath());
                    crate::adapters::mcp_remove_tool(install_root, &mcp_path, &capability.id)?;
                }
                eprintln!(
                    "note: tool '{}' is not MCP-native; copied and tracked without MCP registration",
                    capability.id
                );
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
            capability_type: capability.capability_type,
            installed_version: capability.version.clone(),
            description: capability.description.clone(),
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
    lockfile::prune_unreferenced_baseline_objects(install_root, &lockfile)?;
    Ok(())
}

fn should_print_installed_file(
    capability: &adapter::ResolvedCapability,
    planned: &adapter::PlannedFile,
) -> bool {
    if capability.capability_type != CapabilityType::Skill {
        return true;
    }
    Path::new(&planned.path).file_name() == Some(std::ffi::OsStr::new("SKILL.md"))
}
