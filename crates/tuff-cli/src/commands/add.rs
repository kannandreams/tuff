use std::collections::BTreeMap;
use std::fs;
use std::io::IsTerminal;
use std::path::Path;

use crate::adapter::{
    self, AdapterKind, AgentAdapter, CapabilityKind, HookDefinition, NativeHookConfig,
    resolve_capability,
};
use crate::error::{Result, TuffError};
use crate::git;
use crate::lockfile;
use crate::manifest::{self, CapabilityType, load_manifest};
use crate::release;
use crate::resolver::{self, Scope};

pub use tuff_packs::engine::PolicyOptions;

/// Install one capability with the `tuff` command's baselines, printing
/// what the engine reports when `report` is set.
pub(crate) fn install_capability(
    install_root: &Path,
    scope: Scope,
    capability: &adapter::ResolvedCapability,
    manifest: &manifest::CapabilityManifest,
    target_ids: &[String],
    source: Option<lockfile::CapabilitySource>,
    report: bool,
) -> Result<()> {
    install_capability_accepting(
        install_root,
        scope,
        capability,
        manifest,
        target_ids,
        source,
        report,
        PolicyOptions::default(),
    )
}

/// `install_capability` with the policy choices of [`PolicyOptions`].
#[expect(
    clippy::too_many_arguments,
    reason = "install_capability's context plus the --accept-unenforced choice"
)]
pub(crate) fn install_capability_accepting(
    install_root: &Path,
    scope: Scope,
    capability: &adapter::ResolvedCapability,
    manifest: &manifest::CapabilityManifest,
    target_ids: &[String],
    source: Option<lockfile::CapabilitySource>,
    report: bool,
    policy_options: PolicyOptions,
) -> Result<()> {
    let mut messages = Vec::new();
    let result = tuff_packs::engine::install_capability(
        tuff_packs::engine::Destination {
            root: install_root,
            scope,
            baselines: &super::baselines()?,
        },
        capability,
        manifest,
        target_ids,
        source,
        policy_options,
        &mut messages,
    );
    if report {
        super::print_messages(&messages);
    }
    result
}

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
    cmd_add_accepting(
        repo_root,
        source,
        name,
        capability_type,
        target_ids,
        global,
        hook_file,
        PolicyOptions::default(),
    )
}

/// `tuff add` with the policy choices of [`PolicyOptions`].
#[expect(
    clippy::too_many_arguments,
    reason = "CLI dispatch passes source and install context"
)]
pub fn cmd_add_accepting(
    repo_root: &Path,
    source: Option<&Path>,
    name: Option<&str>,
    capability_type: Option<&str>,
    target_ids: &[String],
    global: bool,
    hook_file: Option<&Path>,
    policy_options: PolicyOptions,
) -> Result<()> {
    let source = source.ok_or_else(|| TuffError::usage("source path or URL is required"))?;
    let (scope, install_root) = if global {
        let home = home_dir()?;
        let lock_path = crate::paths::global_lockfile(&home);
        lockfile::init_lockfile_at(&lock_path)?;
        (Scope::Global, home)
    } else {
        require_project_lockfile(repo_root)?;
        (Scope::Project, repo_root.to_path_buf())
    };
    let target_ids = resolve_agent_selection(&install_root, target_ids, global)?;

    let installed = if git::is_git_url(&source.to_string_lossy()) {
        cmd_add_git(
            &install_root,
            scope,
            &source.to_string_lossy(),
            &target_ids,
            name,
            capability_type,
            repo_root,
            hook_file,
            policy_options,
        )
    } else {
        add_local_source(
            &install_root,
            scope,
            source,
            &target_ids,
            repo_root,
            capability_type,
            name,
            hook_file,
            policy_options,
        )
    };
    installed?;
    note_unreached_harnesses(&install_root, scope, &target_ids, capability_type);
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "CLI dispatch passes source and install context"
)]
fn add_local_source(
    install_root: &Path,
    scope: Scope,
    source: &Path,
    target_ids: &[String],
    repo_root: &Path,
    capability_type: Option<&str>,
    name: Option<&str>,
    hook_file: Option<&Path>,
    policy_options: PolicyOptions,
) -> Result<()> {
    if let Some(name) = name
        && name.contains('@')
    {
        return Err(TuffError::usage(format!(
            "'{name}' carries a version requirement, which applies to git sources only"
        ))
        .with_hint("a local directory has no release tags; pass the name without '@'"));
    }
    cmd_add_local(
        install_root,
        scope,
        source,
        target_ids,
        repo_root,
        capability_type,
        name,
        hook_file,
        policy_options,
    )
}

/// Point out a harness whose files are present in the install root but
/// that the install did not reach. The common case is a Claude Code user
/// installing for `open-agents`: the files land in `.agents/skills`, which
/// Claude Code does not read, so the skill looks installed and never runs.
fn note_unreached_harnesses(
    install_root: &Path,
    scope: Scope,
    target_ids: &[String],
    capability_type: Option<&str>,
) {
    // Skills only: they are the files a harness discovers by folder. MCP
    // servers and policies land in config files a note about folders
    // would misdescribe.
    let kind = capability_type
        .and_then(CapabilityType::parse)
        .unwrap_or(CapabilityType::Skill);
    if kind != CapabilityType::Skill {
        return;
    }
    let covered_agents: Vec<&str> = target_ids
        .iter()
        .filter_map(|id| AdapterKind::from_id(id))
        .flat_map(|adapter| adapter.supported_agents().to_vec())
        .collect();
    let prefix = match scope {
        Scope::Global => "~/",
        Scope::Project => "",
    };
    for adapter in AdapterKind::all() {
        if matches!(adapter, AdapterKind::OpenAgents | AdapterKind::Codex)
            || target_ids.iter().any(|id| id == adapter.id())
            || !adapter.supports(kind)
            || !adapter.detect(install_root)
            || adapter
                .supported_agents()
                .iter()
                .all(|agent| covered_agents.contains(agent))
        {
            continue;
        }
        eprintln!(
            "note: {} reads {prefix}{}/, which this install did not write; add '-a {}' to install for it too",
            adapter.supported_agents().join(", "),
            adapter.dir_prefix(),
            adapter.id()
        );
    }
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
    policy_options: PolicyOptions,
) -> Result<()> {
    // `<name>@<requirement>` asks for a release (RFC-101). The tags are
    // listed before anything is cloned, so a request nothing satisfies
    // costs one round trip and names what was available.
    let (name, request) = match name {
        Some(spec) => {
            let (name, request) = release::split_version_request(spec)?;
            (
                Some(name),
                request.map(release::VersionRequest::parse).transpose()?,
            )
        }
        None => (None, None),
    };
    let source_path = git::source_subdirectory(url);
    let release_name = name.or_else(|| {
        source_path
            .as_deref()
            .and_then(|path| path.rsplit('/').next())
    });
    let release = match &request {
        Some(request) => {
            let release_name = release_name.ok_or_else(|| {
                TuffError::usage(
                    "a version requirement needs a capability name to match release tags against",
                )
                .with_hint(
                    "write <name>@<version>, or use a URL that names the capability's directory",
                )
            })?;
            Some(super::resolve_git_release(url, release_name, request)?)
        }
        None => None,
    };
    let (source_guard, cache_dir, clean_url) = match &release {
        Some(release) => git::clone_tag_to_temp(url, &release.tag)?,
        None => git::clone_to_temp(url, None)?,
    };
    let commit_sha = git::resolve_ref(&cache_dir)?;
    if let (Some(release), Some(request), Some(release_name)) = (&release, &request, release_name) {
        println!(
            "resolved {release_name}@{request} to {} ({})",
            release.tag,
            super::short_sha(&commit_sha)
        );
    }
    let cap_type = capability_type
        .and_then(CapabilityType::parse)
        .unwrap_or(CapabilityType::Skill);
    // The repository-relative path the lockfile records. A path the URL
    // named, or one found by searching, is recorded as is so updates go
    // straight to it; a conventional layout records just the name.
    let mut recorded_path = source_path.clone();
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
    } else if let Some(name) = name {
        let located = git::locate_capability(&cache_dir, name, cap_type)?;
        if located.searched {
            recorded_path = Some(repo_relative(&cache_dir, &located.dir));
        }
        located.dir
    } else {
        // No name and no folder in the URL: a repository with a single
        // capability needs neither, and one with several gets a list to
        // choose from rather than a demand for a flag.
        let found = git::find_capabilities(&cache_dir, cap_type)?;
        match found.as_slice() {
            [only] => {
                recorded_path = Some(only.path.clone());
                cache_dir.join(&only.path)
            }
            [] => {
                return Err(
                    TuffError::not_found(format!("no {cap_type} found in repository"))
                        .with_hint("name the folder in the URL, e.g. <repo>/tree/<branch>/<path>"),
                );
            }
            _ => {
                return Err(TuffError::usage(format!(
                    "repository has {} {}; choose one\n{}",
                    found.len(),
                    cap_type.plural_dir(),
                    git::describe_found(cap_type, &found)
                ))
                .with_hint(format!(
                    "pass a name after the URL, e.g. 'tuff add {cap_type} <url> {}'",
                    found[0].name
                )));
            }
        }
    };

    // The lockfile pins the commit either way. The entry's version is the
    // release when a tag chose it, else what the source declares for itself
    // (RFC-101 tier 2), else the commit SHA, and the scheme says which.
    let installed_version = git_installed_version(&skill_dir, release.as_ref(), &commit_sha);

    let native_hook = cap_type == CapabilityType::Hook && hook_file.is_some();
    let manifest = if native_hook {
        synthetic_local_manifest(&skill_dir, Some(cap_type))?
    } else {
        git_manifest(&skill_dir, name, &installed_version)?
    };
    let name: &str = name.unwrap_or(&manifest.id);
    let source_skill = recorded_path.as_deref().unwrap_or(name);

    let capability = if native_hook {
        resolve_native_hook_capability(&skill_dir, Some(name), &installed_version, hook_file)?
    } else {
        resolve_capability(&manifest)?
    };

    if scope == Scope::Project
        && let Some(warning) = resolver::check_collision(name, project_root, Some(&clean_url))?
    {
        eprintln!("{warning}");
    }

    let result = install_capability_accepting(
        install_root,
        scope,
        &capability,
        &manifest,
        target_ids,
        Some(lockfile::CapabilitySource::Git(lockfile::GitSource {
            url: clean_url,
            path: source_skill.to_string(),
            git_ref: commit_sha,
            tag: release.map(|release| release.tag),
            requested: request.map(|request| request.to_string()),
        })),
        true,
        policy_options,
    );
    drop(source_guard);
    result
}

/// The version a git install records: the release when a tag chose it,
/// else the version the source declares for itself, else the commit.
pub(super) fn git_installed_version(
    skill_dir: &Path,
    release: Option<&release::ReleaseTag>,
    commit_sha: &str,
) -> String {
    match release {
        Some(release) => release.version.to_string(),
        None => manifest::declared_version(skill_dir).unwrap_or_else(|| commit_sha.to_string()),
    }
}

/// The manifest for a capability checked out of git: the real one when the
/// directory has a `tuff.toml`, honouring its type and sections, else one
/// synthesized from the skill directory. `name` overrides the id; the
/// version is always the one the install records.
pub(super) fn git_manifest(
    skill_dir: &Path,
    name: Option<&str>,
    version: &str,
) -> Result<manifest::CapabilityManifest> {
    if skill_dir.join("tuff.toml").is_file() {
        let mut manifest = load_manifest(skill_dir)?;
        if let Some(name) = name {
            manifest::validate_capability_id(name)?;
            manifest.id = name.to_string();
        }
        manifest.version = version.to_string();
        return Ok(manifest);
    }
    // Without a tuff.toml the skill is named after its folder, the same
    // rule `tuff add <local path>` follows.
    let folder_name;
    let name = match name {
        Some(name) => name,
        None => {
            folder_name = skill_dir
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .ok_or_else(|| {
                    TuffError::usage("cannot infer a name for a skill at the repository root")
                        .with_hint("pass a name after the URL, e.g. 'tuff add skill <url> <name>'")
                })?;
            folder_name.as_str()
        }
    };
    manifest::synthetic_manifest(skill_dir, name, version)
}

/// A project-scope add needs the project's `tuff.lock`. Checked before any
/// clone so the error comes first, and worded so it names both ways
/// forward: set up this project, or install for the user with `--global`,
/// which needs no `tuff init` and writes nothing into the project.
fn require_project_lockfile(repo_root: &Path) -> Result<()> {
    let lock_path = lockfile::project_lockfile(repo_root);
    if lock_path.is_file() {
        return Ok(());
    }
    Err(TuffError::not_found(format!(
        "{} has no tuff.lock, so it is not a Tuff project",
        repo_root.display()
    ))
    .with_hint(
        "run 'tuff init' to manage capabilities in this project, or add --global to install for your user instead (no project files, no init needed)",
    ))
}

/// `dir` relative to a checkout root, with `/` separators, as the lockfile
/// records source paths.
fn repo_relative(repo: &Path, dir: &Path) -> String {
    dir.strip_prefix(repo)
        .unwrap_or(dir)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// Adopt or install one local capability directory, the way `tuff add
/// <path>` would.
///
/// `tuff scan --adopt` calls this rather than writing lockfile rows of its
/// own, so there is one implementation of what happens to a local path: the
/// collision warning, the baseline hash, the cache population, and the
/// `Imported` ownership that stops `delete` from removing files Tuff never
/// wrote. Discovery is the only thing scan adds.
pub(crate) fn cmd_add_local_path(
    repo_root: &Path,
    capability_dir: &Path,
    target_id: &str,
    capability_type: CapabilityType,
) -> Result<()> {
    cmd_add_local(
        repo_root,
        Scope::Project,
        capability_dir,
        &[target_id.to_string()],
        repo_root,
        Some(capability_type.as_str()),
        None,
        None,
        PolicyOptions::default(),
    )
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
    policy_options: PolicyOptions,
) -> Result<()> {
    let capability_dir = lockfile::absolutize(install_root, capability_path);
    let parsed_type = capability_type.and_then(CapabilityType::parse);
    let inferred = infer_from_path(&capability_dir);
    let resolved_type = parsed_type.or(Some(inferred.0));
    let mut manifest = load_or_synthetic_manifest(&capability_dir, resolved_type)?;
    if let Some(name) = name {
        manifest::validate_capability_id(name)?;
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

    install_capability_accepting(
        install_root,
        scope,
        &resolved,
        &manifest,
        target_ids,
        None,
        true,
        policy_options,
    )
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
        // What the frontmatter declares, if anything (RFC-101 tier 2).
        version: manifest::declared_version(capability_dir).unwrap_or_else(|| "0.1.0".into()),
        capability_type,
        description: "Added from existing agent assets.".into(),
        files,
        parameters: None,
        implementation: None,
        hook: None,
        server: None,
        policy: None,
        targets: vec![],
        x: Default::default(),
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
        // `is_dir` and `is_file` follow links, so a link inside a hook
        // source would copy whatever it points at into the project, and a
        // link to an ancestor would recurse forever. Refused, as skill and
        // pack sources already refuse it.
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(TuffError::refused(format!(
                "symbolic links are not allowed in capability sources: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            collect_native_hook_source_files_inner(root, &path, hook_path, capability_id, files)?;
            continue;
        }
        if !metadata.is_file() || same_file_path(&path, hook_path) {
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
        Some(".agents" | ".claude" | ".cursor")
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
            managed_permissions: Vec::new(),
            unenforced_rules: Vec::new(),
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
