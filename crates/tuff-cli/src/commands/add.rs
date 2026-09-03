use std::collections::BTreeMap;
use std::fs;
use std::io::IsTerminal;
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

use super::{capability_index, home_dir, infer_from_path, resolve_agent_selection};

pub fn cmd_add(
    repo_root: &Path,
    source: Option<&Path>,
    name: Option<&str>,
    capability_type: Option<&str>,
    target_ids: &[String],
    global: bool,
    hook_file: Option<&Path>,
) -> Result<()> {
    let source = source.ok_or_else(|| TuffError::usage("source path or URL is required"))?;
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

/// `tuff add mcp <source>...` — each source is a built-in catalog id, a
/// local directory holding a `tuff.toml`, or a git URL. Paths and URLs go
/// through the normal typed-add route; catalog ids are synthesized in
/// memory and installed through the same `install_capability` so every
/// lifecycle verb sees an ordinary capability.
pub fn cmd_add_mcp(
    repo_root: &Path,
    sources: &[String],
    target_ids: &[String],
    global: bool,
    yes: bool,
    registry_url: &str,
) -> Result<()> {
    if sources.is_empty() {
        return Err(TuffError::usage(
            "at least one catalog id, path, or git URL is required",
        ));
    }

    for source in sources {
        let as_path = Path::new(source);
        if git::is_git_url(source) || as_path.exists() {
            cmd_add(
                repo_root,
                Some(as_path),
                None,
                Some("mcp-server"),
                target_ids,
                global,
                None,
            )?;
            continue;
        }

        if let Some(mut manifest) = crate::catalog::lookup(source)? {
            prompt_env_overrides(&mut manifest, yes);
            add_catalog_server(repo_root, &manifest, source, None, target_ids, global)?;
            continue;
        }

        // Not a path, a git URL, or a built-in id: ask the registry. A name
        // is matched exactly, so a search hit never installs by surprise.
        let registry = registry_url;
        let Some(server) = super::block_on_oci(crate::registry::fetch(registry, source))? else {
            return Err(TuffError::not_found(format!(
                "'{source}' is not a path, a git URL, a built-in catalog id, or a server in the MCP registry"
            ))
            .with_hint(format!(
                "run 'tuff mcp search {source}' to find its full registry name, or 'tuff add mcp --help' for the built-in ids"
            )));
        };
        let id = crate::registry::default_capability_id(&server.name);
        let mut manifest = crate::registry::to_manifest(&server, &id)?;
        prompt_env_overrides(&mut manifest, yes);
        add_catalog_server(
            repo_root,
            &manifest,
            &server.name,
            Some(registry),
            target_ids,
            global,
        )?;
        // Left out of the manifest because the server does not require
        // them; said out loud so the omission is a choice the user can see
        // and reverse, not a silent one.
        let skipped = crate::registry::skipped_optional_headers(&server);
        if !skipped.is_empty() {
            eprintln!(
                "note: '{}' also documents the optional {} {}; add {} to [server.headers] by hand if you need {}",
                id,
                if skipped.len() == 1 {
                    "header"
                } else {
                    "headers"
                },
                skipped.join(", "),
                if skipped.len() == 1 { "it" } else { "them" },
                if skipped.len() == 1 { "it" } else { "them" },
            );
        }
    }
    Ok(())
}

/// Ask, per variable a server declaration reads, whether to use a different
/// name than the catalog's default — never the secret's value, only which
/// variable holds it, matching the "secrets are references, never values"
/// rule. Header references (RFC-106 D1) join this one flow rather than
/// introducing a second. Only meaningful for catalog installs (a local/git
/// manifest is already fully under the user's own control). Skipped —
/// silently keeping the catalog defaults — with `--yes` or when stdin isn't
/// a real terminal, so this never hangs a script or CI run.
fn prompt_env_overrides(manifest: &mut manifest::CapabilityManifest, yes: bool) {
    if yes || !std::io::stdin().is_terminal() {
        return;
    }
    let Some(server) = manifest.server.as_mut() else {
        return;
    };
    for default in referenced_variables(server) {
        eprint!(
            "{}: reads {default} from your environment. Press enter to keep it, or type a different variable name: ",
            manifest.id
        );
        let _ = std::io::Write::flush(&mut std::io::stderr());
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line).is_err() {
            return;
        }
        if let Some(new_name) = resolve_prompt_answer(&default, &line) {
            rename_env_var(server, &default, &new_name);
        }
    }
}

/// `None` means keep the default (blank input, or input matching the
/// default verbatim); `Some(name)` means use this variable name instead.
fn resolve_prompt_answer(default: &str, line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed == default {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Every variable the server reads, `[server.env]` first and then
/// `[server.headers]`, each name asked about once however many places
/// reference it.
fn referenced_variables(server: &manifest::McpServerConfig) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let referenced = server
        .env
        .values()
        .map(|reference| reference.from_env.clone())
        .chain(
            server
                .headers
                .values()
                .map(|reference| reference.from_env.clone()),
        );
    for name in referenced {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// In `[server.env]` the map key and `from_env` value are always identical
/// today (an entry only ever declares a variable by name, with no separate
/// purpose label), so renaming replaces both. A header is keyed by the
/// header name instead, so only its reference moves, and its `format` is
/// carried across untouched.
fn rename_env_var(server: &mut manifest::McpServerConfig, old_name: &str, new_name: &str) {
    if server.env.remove(old_name).is_some() {
        server.env.insert(
            new_name.to_string(),
            manifest::EnvRef {
                from_env: new_name.to_string(),
            },
        );
    }
    for reference in server.headers.values_mut() {
        if reference.from_env == old_name {
            reference.from_env = new_name.to_string();
        }
    }
}

/// Install one MCP server resolved from a catalog: the one compiled into
/// the binary when `registry` is `None`, otherwise the named MCP registry.
fn add_catalog_server(
    repo_root: &Path,
    manifest: &manifest::CapabilityManifest,
    catalog_id: &str,
    registry: Option<&str>,
    target_ids: &[String],
    global: bool,
) -> Result<()> {
    let (scope, install_root) = if global {
        let home = home_dir()?;
        let lock_path = crate::paths::global_lockfile(&home);
        lockfile::init_lockfile_at(&lock_path)?;
        (Scope::Global, home)
    } else {
        (Scope::Project, repo_root.to_path_buf())
    };
    let target_ids = resolve_agent_selection(&install_root, target_ids, global)?;
    let capability = resolve_capability(manifest)?;

    if scope == Scope::Project
        && let Some(warning) = resolver::check_collision(
            &capability.id,
            repo_root,
            Some(resolver::CATALOG_SOURCE_IDENTITY),
        )?
    {
        eprintln!("{warning}");
    }

    install_capability(
        &install_root,
        scope,
        &capability,
        manifest,
        &target_ids,
        Some(lockfile::CapabilitySource::Catalog(
            lockfile::CatalogSource {
                id: catalog_id.to_string(),
                version: manifest.version.clone(),
                registry: registry.map(str::to_string),
            },
        )),
        true,
    )?;
    match registry {
        Some(registry) => println!(
            "installed {} from {registry} ({} {})",
            capability.id, catalog_id, manifest.version
        ),
        None => println!(
            "installed {} from the built-in catalog (catalog {})",
            capability.id, manifest.version
        ),
    }
    Ok(())
}

/// Whether the lockfile already records `id` for `target` — the one case in
/// which overwriting an existing `mcpServers.<id>` entry is a re-install of
/// our own work rather than clobbering something a person wrote by hand.
pub(crate) fn mcp_entry_tracked(lockfile: &lockfile::Lockfile, id: &str, target: &str) -> bool {
    lockfile
        .capabilities
        .get(id)
        .is_some_and(|entry| entry.targets.contains_key(target))
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
            return Err(TuffError::not_found(format!(
                "capability directory not found in repository: {}",
                path.display()
            )));
        }
        path
    } else if cap_type == CapabilityType::McpServer {
        // No SKILL.md-style discovery exists for servers; the URL must name
        // the directory holding the tuff.toml.
        return Err(TuffError::usage(
            "installing an mcp-server from git requires a subdirectory in the URL (e.g. <repo>//mcp-servers/github)",
        ));
    } else {
        let name = name.ok_or_else(|| {
            TuffError::usage(
                "--name is required when installing from a git URL without a subdirectory",
            )
        })?;
        git::discover_capability(&cache_dir, name, cap_type)?
    };

    let native_hook = cap_type == CapabilityType::Hook && hook_file.is_some();
    let manifest = if native_hook {
        synthetic_local_manifest(&skill_dir, Some(cap_type))?
    } else if skill_dir.join("tuff.toml").is_file() {
        // A real manifest: honour its declared type and sections, but pin
        // the installed version to the commit so `update` can compare refs
        // exactly as it does for synthesized skills.
        let mut manifest = load_manifest(&skill_dir)?;
        if let Some(name) = name {
            manifest.id = name.to_string();
        }
        manifest.version = commit_sha.clone();
        manifest
    } else {
        let name = name.ok_or_else(|| {
            TuffError::usage("--name is required when the git source has no tuff.toml")
        })?;
        manifest::synthetic_manifest(&skill_dir, name, &commit_sha)?
    };
    let name: &str = name.unwrap_or(&manifest.id);
    let source_skill = source_path.as_deref().unwrap_or(name);

    let capability = if native_hook {
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
        Some(lockfile::CapabilitySource::Git(lockfile::GitSource {
            url: clean_url,
            path: source_skill.to_string(),
            git_ref: commit_sha,
            tag: None,
            requested: None,
        })),
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
        // A capability that already lives in a harness layout and is already
        // tracked can still be wanted by a second harness. Emitting it there is
        // an addition to the recorded targets, not a re-adoption, so it does not
        // go through the in-place path and does not disturb the recorded source.
        let tracked = lockfile::require_scoped_lockfile(install_root, scope)?
            .capabilities
            .contains_key(&resolved.id);
        if tracked {
            let added =
                add_missing_targets(install_root, scope, &manifest, &resolved, target_ids, true)?;
            if added == 0 {
                return Err(TuffError::refused(format!(
                    "capability '{}' is already tracked",
                    resolved.id
                ))
                .with_hint(format!(
                    "use 'tuff update {}' for tracked changes",
                    resolved.id
                )));
            }
            return Ok(());
        }
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
        return Err(TuffError::usage(
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
        return Err(TuffError::not_found(format!(
            "directory not found: {}",
            capability_dir.display()
        )));
    }

    let id = capability_dir
        .file_name()
        .ok_or_else(|| TuffError::usage("capability directory must have a name"))?
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
        return Err(TuffError::usage(format!(
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
        server: None,
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
    let hook_file = hook_file.ok_or_else(|| TuffError::usage("--hook-file is required"))?;
    let id = name
        .map(str::to_string)
        .or_else(|| {
            capability_dir
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .ok_or_else(|| TuffError::usage("hook directory must have a name"))?;

    let hook_path = if hook_file.is_absolute() {
        hook_file.to_path_buf()
    } else {
        capability_dir.join(hook_file)
    };
    if !hook_path.is_file() {
        return Err(TuffError::not_found(format!(
            "hook fragment not found: {}",
            hook_path.display()
        )));
    }
    let hook_rel = hook_path
        .strip_prefix(capability_dir)
        .map_err(|_| TuffError::usage("--hook-file must be inside the hook source directory"))?
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
    manifest: &manifest::CapabilityManifest,
    capability: &adapter::ResolvedCapability,
    inferred_target: &str,
    target_ids: &[String],
) -> Result<()> {
    for target_id in target_ids {
        if target_id != inferred_target {
            return Err(TuffError::usage(format!(
                "{} is already in the '{}' agent layout; use -a {}",
                lockfile::relative_or_absolute_fs(capability_dir, install_root),
                inferred_target,
                inferred_target
            )));
        }
    }

    let mut lockfile = lockfile::require_scoped_lockfile(install_root, scope)?;
    if lockfile.capabilities.contains_key(&capability.id) {
        return Err(TuffError::refused(format!(
            "capability '{}' is already tracked",
            capability.id
        ))
        .with_hint(format!(
            "use 'tuff update {}' for tracked changes",
            capability.id
        )));
    }

    let mut targets = BTreeMap::new();
    let installed_root = capability_dir.to_path_buf();
    let baseline_hash = crate::cache::hash_tree(&installed_root)?;
    crate::cache::populate(&super::home_dir()?, &baseline_hash, &installed_root)?;
    targets.insert(
        inferred_target.to_string(),
        lockfile::TargetLockEntry {
            managed_hooks: Vec::new(),
            managed_mcp_entry: None,
            ownership: lockfile::TargetOwnership::Imported,
            sha256: baseline_hash,
            installed_path: relative_or_absolute_canonical(capability_dir, install_root),
        },
    );

    lockfile.capabilities.insert(
        capability.id.clone(),
        lockfile::CapabilityLockEntry {
            capability_type: capability.capability_type,
            version: capability.version.clone(),
            version_scheme: lockfile::VersionScheme::Declared,
            description: capability.description.clone(),
            source: lockfile::CapabilitySource::local(relative_or_absolute_canonical(
                capability_dir,
                install_root,
            )),
            targets,
            implementation: manifest.implementation.clone(),
            parameters: manifest.parameters.clone(),
            workflow: manifest.workflow.clone(),
            server: manifest.server.clone(),
        },
    );
    lockfile::write_scoped_lockfile(install_root, scope, &lockfile)?;
    capability_index::regenerate_capability_index(install_root, scope)?;
    println!(
        "added {} ({}, {}) -> {}",
        capability.id,
        capability.capability_type,
        inferred_target,
        relative_or_absolute_canonical(capability_dir, install_root)
    );
    Ok(())
}

/// Write one planned file to disk and record it as an `EmittedFile`.
///
/// Shared by `install_capability`'s write phase and the generated
/// capability-index skill (`capability_index.rs`), which has no hooks or
/// reporting to layer on top — just files to write and hash.
pub(crate) fn write_planned_file(
    install_root: &Path,
    planned: &adapter::PlannedFile,
) -> Result<adapter::EmittedFile> {
    let target_path = install_root.join(&planned.path);
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target_path, &planned.content)?;

    Ok(adapter::EmittedFile {
        path: planned.path.clone(),
        hash: lockfile::hash_bytes(&planned.content),
        baseline_hash: lockfile::hash_bytes(&planned.content),
    })
}

/// Emits an already-tracked capability into harnesses it is not yet recorded
/// for, and records those targets. Returns how many targets were added.
///
/// The recorded source, version, and description belong to the original
/// install, so they are restored afterwards: adding a harness says nothing new
/// about where the capability came from. A target that is already recorded is
/// skipped rather than re-emitted, which keeps the call idempotent.
pub(crate) fn add_missing_targets(
    install_root: &Path,
    scope: Scope,
    manifest: &manifest::CapabilityManifest,
    capability: &adapter::ResolvedCapability,
    target_ids: &[String],
    report: bool,
) -> Result<usize> {
    let lockfile = lockfile::require_scoped_lockfile(install_root, scope)?;
    let existing = lockfile
        .capabilities
        .get(&capability.id)
        .ok_or_else(|| {
            TuffError::not_found(format!("capability '{}' is not tracked", capability.id))
        })?
        .clone();

    let missing: Vec<String> = target_ids
        .iter()
        .filter(|target_id| !existing.targets.contains_key(*target_id))
        .cloned()
        .collect();
    if missing.is_empty() {
        return Ok(0);
    }

    install_capability(
        install_root,
        scope,
        capability,
        manifest,
        &missing,
        Some(existing.source.clone()),
        report,
    )?;

    let mut lockfile = lockfile::require_scoped_lockfile(install_root, scope)?;
    if let Some(entry) = lockfile.capabilities.get_mut(&capability.id) {
        entry.version = existing.version;
        entry.version_scheme = existing.version_scheme;
        entry.description = existing.description;
        entry.implementation = existing.implementation;
        entry.parameters = existing.parameters;
        entry.workflow = existing.workflow;
        entry.server = existing.server;
    }
    lockfile::write_scoped_lockfile(install_root, scope, &lockfile)?;

    Ok(missing.len())
}

/// Emits a tracked capability, read from the directory it is installed in, into
/// additional harnesses. Used by `tuff init` to give a detected harness the CLI
/// guide it would otherwise never see.
pub(crate) fn add_targets_from_installed_dir(
    install_root: &Path,
    scope: Scope,
    capability_dir: &Path,
    target_ids: &[String],
    report: bool,
) -> Result<usize> {
    let inferred = infer_from_path(capability_dir);
    let manifest = load_or_synthetic_manifest(capability_dir, Some(inferred.0))?;
    let resolved = resolve_capability(&manifest)?;
    add_missing_targets(
        install_root,
        scope,
        &manifest,
        &resolved,
        target_ids,
        report,
    )
}

pub(crate) fn install_capability(
    install_root: &Path,
    scope: Scope,
    capability: &adapter::ResolvedCapability,
    manifest: &manifest::CapabilityManifest,
    target_ids: &[String],
    source: Option<lockfile::CapabilitySource>,
    report: bool,
) -> Result<()> {
    let mut adapters = Vec::new();
    for tid in target_ids {
        let adapter = AdapterKind::from_id(tid).ok_or_else(|| {
            TuffError::usage(format!("unknown agent '{}'", tid,))
                .with_hint("run 'tuff agent list' to see available agents")
        })?;
        if !adapter.supports(capability.capability_type) {
            return Err(TuffError::unsupported(format!(
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

    let lockfile = lockfile::require_scoped_lockfile(install_root, scope)?;
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
                    return Err(TuffError::refused(format!(
                        "refusing to overwrite untracked file at {}",
                        lockfile::relative_or_absolute_fs(&target_path, install_root)
                    ))
                    .with_hint("remove it, or track it in Tuff first"));
                }
            }
        }
    }

    if matches!(
        capability.kind,
        CapabilityKind::Tool { .. } | CapabilityKind::McpServer { .. }
    ) {
        for adapter in &adapters {
            let mcp_path = install_root.join(adapter.mcp_config_relpath());
            tuff_core::mcp::validate_config(&mcp_path)?;

            // For an external server the JSON entry *is* the product, so an
            // entry Tuff never wrote is a collision — refuse here, before a
            // single file lands, rather than after `server.toml` is written.
            if matches!(capability.kind, CapabilityKind::McpServer { .. })
                && !mcp_entry_tracked(&lockfile, &capability.id, adapter.id())
                && tuff_core::mcp::has_server(&mcp_path, &capability.id)?
            {
                return Err(TuffError::refused(format!(
                    "refusing to overwrite untracked MCP server '{}' in {}",
                    capability.id,
                    lockfile::relative_or_absolute_fs(&mcp_path, install_root)
                ))
                .with_hint("remove it by hand, or choose a different capability id"));
            }
        }
    }

    let mut new_targets: BTreeMap<String, TargetLockEntry> = BTreeMap::new();

    for (adapter, planned_files) in &plans {
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
            write_planned_file(install_root, planned)?;

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
                managed_hooks,
                managed_mcp_entry: None,
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
                // MCP-native tools get the same per-entry baseline as
                // external servers: the entry shape below must match what
                // `mcp::register_tool` writes.
                let entry_value = serde_json::json!({"command": mcp_command, "args": mcp_args});
                if let Some(target) = new_targets.get_mut(adapter.id()) {
                    target.managed_mcp_entry = Some(lockfile::ManagedMcpEntry {
                        config_path: adapter.mcp_config_relpath().to_string(),
                        baseline_hash: lockfile::managed_mcp_entry_baseline(&entry_value)?,
                    });
                }
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

    if let CapabilityKind::McpServer { ref server } = capability.kind {
        for adapter in &adapters {
            let mcp_path = install_root.join(adapter.mcp_config_relpath());
            let tracked = mcp_entry_tracked(&lockfile, &capability.id, adapter.id());
            let entry_value = adapter.mcp_server_entry(server);
            let baseline = lockfile::managed_mcp_entry_baseline(&entry_value)?;
            crate::adapters::mcp_register_server(
                install_root,
                &mcp_path,
                &capability.id,
                entry_value,
                tracked,
            )?;
            if let Some(target) = new_targets.get_mut(adapter.id()) {
                target.managed_mcp_entry = Some(lockfile::ManagedMcpEntry {
                    config_path: adapter.mcp_config_relpath().to_string(),
                    baseline_hash: baseline,
                });
            }
            if report {
                println!(
                    "registered MCP server {} ({}) -> {}",
                    capability.id,
                    adapter.id(),
                    lockfile::relative_or_absolute_fs(&mcp_path, install_root)
                );
            }
        }
        let required = crate::catalog::required_env(server);
        if report && !required.is_empty() {
            eprintln!(
                "note: '{}' reads {} from the environment; export {} before starting the harness",
                capability.id,
                if required.len() == 1 {
                    "a variable"
                } else {
                    "variables"
                },
                required.join(", ")
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

    let source = source.unwrap_or_else(|| {
        lockfile::CapabilitySource::local(lockfile::relative_or_absolute_fs(
            &manifest.root,
            install_root,
        ))
    });

    lockfile.capabilities.insert(
        capability.id.clone(),
        lockfile::CapabilityLockEntry {
            capability_type: capability.capability_type,
            version: capability.version.clone(),
            version_scheme: source.default_version_scheme(),
            description: capability.description.clone(),
            source,
            targets: merged_targets,
            implementation: manifest.implementation.clone(),
            parameters: manifest.parameters.clone(),
            workflow: manifest.workflow.clone(),
            server: manifest.server.clone(),
        },
    );

    lockfile::write_scoped_lockfile(install_root, scope, &lockfile)?;
    capability_index::regenerate_capability_index(install_root, scope)?;
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
    use super::{normalize_native_hook_path, rename_env_var, resolve_prompt_answer};
    use crate::manifest::{EnvRef, McpServerConfig, McpTransport};

    #[test]
    fn resolve_prompt_answer_keeps_default_on_blank_or_matching_input() {
        assert_eq!(resolve_prompt_answer("GITHUB_TOKEN", "\n"), None);
        assert_eq!(resolve_prompt_answer("GITHUB_TOKEN", "   \n"), None);
        assert_eq!(
            resolve_prompt_answer("GITHUB_TOKEN", "GITHUB_TOKEN\n"),
            None
        );
    }

    #[test]
    fn resolve_prompt_answer_returns_a_trimmed_different_name() {
        assert_eq!(
            resolve_prompt_answer("GITHUB_TOKEN", "  GH_TOKEN  \n"),
            Some("GH_TOKEN".to_string())
        );
    }

    #[test]
    fn rename_env_var_replaces_both_the_key_and_the_reference() {
        let mut server = McpServerConfig {
            transport: McpTransport::Stdio,
            command: Some("npx".to_string()),
            args: Vec::new(),
            url: None,
            env: std::collections::BTreeMap::from([(
                "GITHUB_TOKEN".to_string(),
                EnvRef {
                    from_env: "GITHUB_TOKEN".to_string(),
                },
            )]),
            headers: std::collections::BTreeMap::new(),
            metadata: None,
        };
        rename_env_var(&mut server, "GITHUB_TOKEN", "GH_TOKEN");
        assert!(!server.env.contains_key("GITHUB_TOKEN"));
        assert_eq!(server.env["GH_TOKEN"].from_env, "GH_TOKEN");
    }

    #[test]
    fn rename_env_var_is_a_no_op_for_an_unknown_name() {
        let mut server = McpServerConfig {
            transport: McpTransport::Stdio,
            command: Some("npx".to_string()),
            args: Vec::new(),
            url: None,
            env: std::collections::BTreeMap::new(),
            headers: std::collections::BTreeMap::new(),
            metadata: None,
        };
        rename_env_var(&mut server, "NOT_PRESENT", "OTHER");
        assert!(server.env.is_empty());
    }

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
