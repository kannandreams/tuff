use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::adapter::{self, AdapterKind, resolve_capability, AgentAdapter};
use crate::error::{CoralError, Result};
use crate::git;
use crate::lockfile::{self, TargetLockEntry};
use crate::manifest::{self, load_manifest, CapabilityType};
use crate::resolver::{self, Scope};

use super::{home_dir, infer_from_path, resolve_agent_selection, toml_escape};

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
    let target_ids = resolve_agent_selection(&install_root, target_ids)?;

    if git::is_git_url(&capability.to_string_lossy()) {
        return cmd_add_git(
            &install_root,
            scope,
            &capability.to_string_lossy(),
            &target_ids,
            skill_name,
            tool_name,
            hook_name,
            repo_root,
        );
    }
    cmd_add_local(&install_root, scope, capability, &target_ids, repo_root)
}

#[allow(clippy::too_many_arguments)]
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
            source_ref: commit_sha.clone(),
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
) -> Result<()> {
    let capability_dir = lockfile::absolutize(install_root, capability_path);
    let inferred = infer_from_path(&capability_dir);
    let manifest = load_or_synthetic_manifest(&capability_dir, Some(inferred.0))?;
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
        if capability.capability_type == CapabilityType::Hook
            && let Some(ref hook_cfg) = capability.hook
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
        let mut planned = adapter.plan(capability, install_root)?;
        if let Some(manifest_file) =
            copied_tool_manifest_file(*adapter, install_root, capability, manifest)?
        {
            planned.push(manifest_file);
        }
        if is_git && capability.capability_type == CapabilityType::Skill {
            planned.push(generated_git_manifest_file(
                *adapter,
                install_root,
                capability,
            ));
        }
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

            if is_generated_manifest_file(planned) {
                continue;
            }

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

    if capability.capability_type == CapabilityType::Tool
        && let Some(ref impl_cfg) = capability.implementation {
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

fn generated_git_manifest_file(
    adapter: AdapterKind,
    install_root: &Path,
    capability: &adapter::ResolvedCapability,
) -> adapter::PlannedFile {
    let dir = install_root.join(adapter.dir_prefix()).join("skills");
    let path = dir.join(&capability.id).join("coral.toml");
    let description = if capability.description.trim().is_empty() {
        "Installed from git source."
    } else {
        &capability.description
    };
    let content = format!(
        "# Generated by coral add\nid = \"{}\"\nversion = \"{}\"\ntype = \"skill\"\ndescription = \"{}\"\nfiles = [{}]\n",
        toml_escape(&capability.id),
        toml_escape(&capability.version),
        toml_escape(description),
        capability
            .source_files
            .iter()
            .map(|(name, _)| format!("\"{}\"", toml_escape(name)))
            .collect::<Vec<_>>()
            .join(", ")
    );

    adapter::PlannedFile {
        path: lockfile::relative_or_absolute_fs(&path, install_root),
        content: content.into_bytes(),
    }
}

fn copied_tool_manifest_file(
    adapter: AdapterKind,
    install_root: &Path,
    capability: &adapter::ResolvedCapability,
    manifest: &manifest::CapabilityManifest,
) -> Result<Option<adapter::PlannedFile>> {
    if capability.capability_type != CapabilityType::Tool {
        return Ok(None);
    }

    let manifest_path = manifest.root.join("coral.toml");
    if !manifest_path.exists() {
        return Ok(None);
    }

    let dir = install_root.join(adapter.dir_prefix()).join("tools");
    let path = dir.join(&capability.id).join("coral.toml");

    Ok(Some(adapter::PlannedFile {
        path: lockfile::relative_or_absolute_fs(&path, install_root),
        content: fs::read(manifest_path)?,
    }))
}

fn is_generated_manifest_file(planned: &adapter::PlannedFile) -> bool {
    Path::new(&planned.path).file_name() == Some(std::ffi::OsStr::new("coral.toml"))
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
