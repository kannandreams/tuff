use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::adapter::{
    self, AdapterKind, AgentAdapter, CapabilityKind, HookDefinition, HookRenderContext,
    NativeHookConfig, resolve_capability,
};
use crate::error::{Result, TuffError};
use crate::git;
use crate::lockfile::{self, TargetLockEntry};
use crate::manifest::{self, CapabilityType, load_manifest};
use crate::resolver::{self, Scope};

use super::{home_dir, infer_from_path, resolve_agent_selection};

pub fn cmd_add(
    repo_root: &Path,
    source: Option<&Path>,
    name: Option<&str>,
    capability_type: Option<&str>,
    target_ids: &[String],
    global: bool,
    hook_file: Option<&Path>,
) -> Result<()> {
    let source = source.ok_or_else(|| TuffError::new("source path or URL is required"))?;
    let (scope, install_root) = if global {
        let home = home_dir()?;
        let lock_path = crate::paths::global_lockfile(&home);
        lockfile::init_lockfile_at(&lock_path)?;
        (Scope::Global, home)
    } else {
        (Scope::Project, repo_root.to_path_buf())
    };
    let target_ids = resolve_agent_selection(&install_root, target_ids, global)?;

    if git::is_git_url(&source.to_string_lossy()) {
        return cmd_add_git(
            &install_root,
            scope,
            &source.to_string_lossy(),
            &target_ids,
            name,
            capability_type,
            repo_root,
            hook_file,
        );
    }
    cmd_add_local(
        &install_root,
        scope,
        source,
        &target_ids,
        repo_root,
        capability_type,
        name,
        hook_file,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "CLI dispatch passes source and install context"
)]
fn cmd_add_git(
    install_root: &Path,
    scope: Scope,
    url: &str,
    target_ids: &[String],
    name: Option<&str>,
    capability_type: Option<&str>,
    project_root: &Path,
    hook_file: Option<&Path>,
) -> Result<()> {
    let name =
        name.ok_or_else(|| TuffError::new("--name is required when installing from a git URL"))?;

    let (source_guard, cache_dir, clean_url) = git::clone_to_temp(url, None)?;
    let source_path = git::source_subdirectory(url);
    let commit_sha = git::resolve_ref(&cache_dir)?;
    let cap_type = capability_type
        .and_then(CapabilityType::parse)
        .unwrap_or(CapabilityType::Skill);
    let skill_dir = if let Some(path) = source_path.as_deref() {
        crate::tool::check_path_traversal(path)?;
        let path = cache_dir.join(path);
        if !path.is_dir() {
            return Err(TuffError::new(format!(
                "capability directory not found in repository: {}",
                path.display()
            )));
        }
        path
    } else {
        git::discover_capability(&cache_dir, name, cap_type)?
    };
    let source_skill = source_path.as_deref().unwrap_or(name);

    let manifest = if cap_type == CapabilityType::Hook && hook_file.is_some() {
        synthetic_local_manifest(&skill_dir, Some(cap_type))?
    } else {
        manifest::synthetic_manifest(&skill_dir, name, &commit_sha)?
    };
    let capability = if cap_type == CapabilityType::Hook && hook_file.is_some() {
        resolve_native_hook_capability(&skill_dir, Some(name), &commit_sha, hook_file)?
    } else {
        resolve_capability(&manifest)?
    };

    if scope == Scope::Project
        && let Some(warning) = resolver::check_collision(name, project_root, Some(&clean_url))?
    {
        eprintln!("{warning}");
    }

    let result = install_capability(
        install_root,
        scope,
        &capability,
        &manifest,
        target_ids,
        Some(&SourceMetaInput {
            source_type: "git".to_string(),
            url: clean_url,
            source_ref: commit_sha,
            skill: source_skill.to_string(),
        }),
        true,
    );
    drop(source_guard);
    result
}

#[expect(
    clippy::too_many_arguments,
    reason = "CLI dispatch passes source and install context"
)]
fn cmd_add_local(
    install_root: &Path,
    scope: Scope,
    capability_path: &Path,
    target_ids: &[String],
    project_root: &Path,
    capability_type: Option<&str>,
    name: Option<&str>,
    hook_file: Option<&Path>,
) -> Result<()> {
    let capability_dir = lockfile::absolutize(install_root, capability_path);
    let parsed_type = capability_type.and_then(CapabilityType::parse);
    let inferred = infer_from_path(&capability_dir);
    let resolved_type = parsed_type.or(Some(inferred.0));
    let mut manifest = load_or_synthetic_manifest(&capability_dir, resolved_type)?;
    if let Some(name) = name {
        validate_capability_name(name)?;
        manifest.id = name.to_string();
    }
    let resolved = if resolved_type == Some(CapabilityType::Hook) && hook_file.is_some() {
        resolve_native_hook_capability(&capability_dir, name, "0.1.0", hook_file)?
    } else {
        resolve_capability(&manifest)?
    };

    if scope == Scope::Project
        && let Some(warning) = resolver::check_collision(&resolved.id, project_root, None)?
    {
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

    install_capability(
        install_root,
        scope,
        &resolved,
        &manifest,
        target_ids,
        None,
        true,
    )
}

fn validate_capability_name(name: &str) -> Result<()> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
        return Err(TuffError::new(
            "capability name must be a non-empty single path component",
        ));
    }
    Ok(())
}

fn load_or_synthetic_manifest(
    capability_dir: &Path,
    inferred_type: Option<CapabilityType>,
) -> Result<manifest::CapabilityManifest> {
    if capability_dir.join("tuff.toml").exists() {
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
        return Err(TuffError::new(format!(
            "directory not found: {}",
            capability_dir.display()
        )));
    }

    let id = capability_dir
        .file_name()
        .ok_or_else(|| TuffError::new("capability directory must have a name"))?
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
        if name != "tuff.toml" {
            files.push(name);
        }
    }
    files.sort();
    if files.is_empty() {
        return Err(TuffError::new(format!(
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

fn resolve_native_hook_capability(
    capability_dir: &Path,
    name: Option<&str>,
    version: &str,
    hook_file: Option<&Path>,
) -> Result<adapter::ResolvedCapability> {
    let hook_file = hook_file.ok_or_else(|| TuffError::new("--hook-file is required"))?;
    let id = name
        .map(str::to_string)
        .or_else(|| {
            capability_dir
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .ok_or_else(|| TuffError::new("hook directory must have a name"))?;

    let hook_path = if hook_file.is_absolute() {
        hook_file.to_path_buf()
    } else {
        capability_dir.join(hook_file)
    };
    if !hook_path.is_file() {
        return Err(TuffError::new(format!(
            "hook fragment not found: {}",
            hook_path.display()
        )));
    }
    let hook_rel = hook_path
        .strip_prefix(capability_dir)
        .map_err(|_| TuffError::new("--hook-file must be inside the hook source directory"))?
        .to_string_lossy()
        .replace('\\', "/");
    crate::tool::check_path_traversal(&hook_rel)?;

    let fragment: serde_json::Value = serde_json::from_str(&fs::read_to_string(&hook_path)?)?;
    let source_files = collect_native_hook_source_files(capability_dir, &hook_path, &id)?;

    Ok(adapter::ResolvedCapability {
        id,
        capability_type: CapabilityType::Hook,
        version: version.to_string(),
        description: "Added from native hook fragment.".into(),
        source_files: source_files.clone(),
        source_dir: capability_dir.to_path_buf(),
        kind: CapabilityKind::Hook {
            hook: HookDefinition::Native(NativeHookConfig {
                fragment,
                source_files,
            }),
        },
    })
}

fn collect_native_hook_source_files(
    capability_dir: &Path,
    hook_path: &Path,
    capability_id: &str,
) -> Result<Vec<(String, Vec<u8>)>> {
    let mut files = Vec::new();
    collect_native_hook_source_files_inner(
        capability_dir,
        capability_dir,
        hook_path,
        capability_id,
        &mut files,
    )?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    if files.is_empty() {
        eprintln!(
            "note: hook source has no runtime files; only the native hook fragment will be merged"
        );
    }
    Ok(files)
}

fn collect_native_hook_source_files_inner(
    root: &Path,
    current: &Path,
    hook_path: &Path,
    capability_id: &str,
    files: &mut Vec<(String, Vec<u8>)>,
) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_native_hook_source_files_inner(root, &path, hook_path, capability_id, files)?;
            continue;
        }
        if !path.is_file() || same_file_path(&path, hook_path) {
            continue;
        }

        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if rel == "tuff.toml" {
            continue;
        }
        let Some(rel) = normalize_native_hook_path(&rel, capability_id) else {
            continue;
        };
        files.push((rel, fs::read(&path)?));
    }
    Ok(())
}

fn normalize_native_hook_path(path: &str, capability_id: &str) -> Option<String> {
    const SOURCE_HOOK_ROOTS: &[&str] = &[
        ".codex/hooks/",
        ".claude/hooks/",
        ".agents/hooks/",
        "hooks/",
    ];

    SOURCE_HOOK_ROOTS
        .iter()
        .find_map(|prefix| path.strip_prefix(prefix).map(str::to_string))
        .or_else(|| Some(path.to_string()))
        .map(|path| {
            path.strip_prefix(&format!("{capability_id}/"))
                .unwrap_or(&path)
                .to_string()
        })
        .filter(|path| !path.is_empty())
}

fn same_file_path(left: &Path, right: &Path) -> bool {
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
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
            return Err(TuffError::new(format!(
                "{} is already in the '{}' agent layout; use -a {}",
                lockfile::relative_or_absolute_fs(capability_dir, install_root),
                inferred_target,
                inferred_target
            )));
        }
    }

    let mut lockfile = lockfile::require_lockfile(install_root)?;
    if lockfile.capabilities.contains_key(&capability.id) {
        return Err(TuffError::new(format!(
            "capability '{}' is already tracked; use 'tuff update {}' for tracked changes",
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
    let installed_root = capability_dir.to_path_buf();
    let baseline_hash = crate::cache::hash_tree(&installed_root)?;
    crate::cache::populate(&super::home_dir()?, &baseline_hash, &installed_root)?;
    targets.insert(
        inferred_target.to_string(),
        lockfile::TargetLockEntry {
            emitted_files,
            managed_hooks: Vec::new(),
            ownership: lockfile::TargetOwnership::Imported,
            sha256: baseline_hash,
            installed_path: relative_or_absolute_canonical(capability_dir, install_root),
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
            pack: None,
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
    report: bool,
) -> Result<()> {
    let is_git = source_meta.is_some();

    let mut adapters = Vec::new();
    for tid in target_ids {
        let adapter = AdapterKind::from_id(tid).ok_or_else(|| {
            TuffError::new(format!(
                "unknown agent '{}'; run 'tuff agent list' to see available agents",
                tid
            ))
        })?;
        if !adapter.supports(capability.capability_type) {
            return Err(TuffError::new(format!(
                "{} does not yet support {} capabilities",
                adapter.display_name(),
                capability.capability_type
            )));
        }
        if let CapabilityKind::Hook {
            hook: HookDefinition::Command(ref hook_cfg),
        } = capability.kind
        {
            adapter.native_hook_event(&hook_cfg.event)?;
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
                if f.allow_existing {
                    continue;
                }
                let target_path = install_root.join(&f.path);
                if target_path.exists() {
                    return Err(TuffError::new(format!(
                        "refusing to overwrite untracked file at {}; remove it or track it in Tuff first",
                        lockfile::relative_or_absolute_fs(&target_path, install_root)
                    )));
                }
            }
        }
    }

    if matches!(capability.kind, CapabilityKind::Tool { .. }) {
        for adapter in &adapters {
            let mcp_path = install_root.join(adapter.mcp_config_relpath());
            tuff_core::mcp::validate_config(&mcp_path)?;
        }
    }

    let mut new_targets: BTreeMap<String, TargetLockEntry> = BTreeMap::new();

    for (adapter, planned_files) in &plans {
        let mut emitted = Vec::new();
        let mut managed_hooks = Vec::new();

        if let CapabilityKind::Hook { hook } = &capability.kind {
            let hook_root = install_root
                .join(adapter.dir_prefix())
                .join("hooks")
                .join(&capability.id);
            let hook_root_rel = lockfile::relative_or_absolute_fs(&hook_root, install_root);
            match hook {
                HookDefinition::Command(hook_cfg) => {
                    let render = adapter.render_standard_hook(HookRenderContext {
                        capability_id: &capability.id,
                        hook: hook_cfg,
                        source_files: &capability.source_files,
                        repo_root: install_root,
                        track_managed_hooks: true,
                    })?;
                    if report {
                        for diagnostic in render.diagnostics {
                            eprintln!("{}", diagnostic.message);
                        }
                    }
                    managed_hooks = render.managed_hooks;
                }
                HookDefinition::Native(native) => {
                    let fragment = adapter::replace_hook_dir_placeholder(
                        native.fragment.clone(),
                        &hook_root_rel,
                    );
                    let settings_path = adapter.hook_settings_relpath();
                    managed_hooks = lockfile::managed_hooks_from_fragment(
                        install_root,
                        settings_path,
                        &fragment,
                    )?;
                }
            }
        }

        for planned in planned_files {
            let target_path = install_root.join(&planned.path);
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target_path, &planned.content)?;

            let hash = lockfile::hash_bytes(&planned.content);
            let shared_settings = managed_hooks
                .iter()
                .any(|hook| hook.settings_path == planned.path);
            if !shared_settings {
                emitted.push(adapter::EmittedFile {
                    path: planned.path.clone(),
                    hash,
                    baseline_hash: lockfile::write_baseline_object(install_root, &planned.content)?,
                });
            }

            if report && should_print_installed_file(capability, planned) {
                println!(
                    "installed {} ({}) -> {}",
                    capability.id,
                    adapter.id(),
                    lockfile::relative_or_absolute_fs(&target_path, install_root)
                );
            }
        }

        let installed_root = install_root
            .join(adapter.dir_prefix())
            .join(capability.capability_type.plural_dir())
            .join(&capability.id);
        let baseline_hash = crate::cache::hash_tree(&installed_root)?;
        crate::cache::populate(&super::home_dir()?, &baseline_hash, &installed_root)?;
        new_targets.insert(
            adapter.id().to_string(),
            TargetLockEntry {
                emitted_files: emitted,
                managed_hooks,
                ownership: target_ownership_for(capability, install_root, *adapter),
                sha256: baseline_hash,
                installed_path: lockfile::relative_or_absolute_fs(&installed_root, install_root),
            },
        );
    }

    if let CapabilityKind::Tool {
        implementation: ref impl_cfg,
        ..
    } = capability.kind
    {
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
                if report {
                    println!(
                        "registered MCP server {} ({}) -> {}",
                        capability.id,
                        adapter.id(),
                        lockfile::relative_or_absolute_fs(&mcp_path, install_root)
                    );
                }
            }
        } else {
            for adapter in &adapters {
                let mcp_path = install_root.join(adapter.mcp_config_relpath());
                crate::adapters::mcp_remove_tool(install_root, &mcp_path, &capability.id)?;
            }
            if report {
                eprintln!(
                    "note: tool '{}' is not MCP-native; copied and tracked without MCP registration",
                    capability.id
                );
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
            pack: None,
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

fn target_ownership_for(
    capability: &adapter::ResolvedCapability,
    install_root: &Path,
    adapter: AdapterKind,
) -> lockfile::TargetOwnership {
    if matches!(
        &capability.kind,
        CapabilityKind::Hook {
            hook: HookDefinition::Native(_)
        }
    ) && is_path_under(
        &capability.source_dir,
        &install_root.join(adapter.dir_prefix()),
    ) {
        lockfile::TargetOwnership::Imported
    } else {
        lockfile::TargetOwnership::Generated
    }
}

fn is_path_under(path: &Path, root: &Path) -> bool {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical_path.starts_with(canonical_root)
}

#[cfg(test)]
mod tests {
    use super::normalize_native_hook_path;

    #[test]
    fn native_hook_paths_strip_source_harness_roots() {
        assert_eq!(
            normalize_native_hook_path(".codex/hooks/change-logger/run.sh", "change-logger"),
            Some("run.sh".to_string())
        );
        assert_eq!(
            normalize_native_hook_path(".claude/hooks/run.sh", "change-logger"),
            Some("run.sh".to_string())
        );
        assert_eq!(
            normalize_native_hook_path("hooks/run.sh", "change-logger"),
            Some("run.sh".to_string())
        );
    }

    #[test]
    fn native_hook_paths_preserve_runtime_paths() {
        assert_eq!(
            normalize_native_hook_path("config/change-logger.json", "change-logger"),
            Some("config/change-logger.json".to_string())
        );
    }
}
