use std::{
    collections::BTreeMap,
    path::Path,
};

use tabled::{
    settings::{object::Columns, Modify, Style, Width},
    Table, Tabled,
};

use crate::{
    adapter::{self, AdapterKind, resolve_primitive},
    config, git,
    error::{CoralError, Result},
    lockfile::{self, TargetLockEntry},
    manifest::{self, load_manifest},
    resolver::{self, Scope},
};

pub fn cmd_init(repo_root: &Path, global: bool) -> Result<()> {
    if global {
        let home = home_dir()?;
        let lock_path = home.join(".coral").join("coral-lock.json");
        lockfile::init_lockfile_at(&lock_path)?;
        println!("initialized ~/.coral/coral-lock.json");
    } else {
        let lock_path = lockfile::init_lockfile(repo_root)?;
        let _ = config::read_config(repo_root)?;
        println!(
            "initialized {}",
            lockfile::relative_or_absolute_fs(&lock_path, repo_root)
        );
    }
    Ok(())
}

// ── Add ─────────────────────────────────────────────────────────────────────

pub fn cmd_add(
    repo_root: &Path,
    capability: &Path,
    target_ids: &[String],
    skill_name: Option<&str>,
    tool_name: Option<&str>,
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
    project_root: &Path,
) -> Result<()> {
    let name = skill_name.or(tool_name).ok_or_else(|| {
        CoralError::new("--skill or --tool is required when installing from a git URL")
    })?;

    let (cache_dir, clean_url) = git::clone_or_fetch(url)?;
    let commit_sha = git::resolve_ref(&cache_dir)?;
    let skill_dir = git::discover_skill(&cache_dir, name)?;

    let manifest = manifest::synthetic_manifest(&skill_dir, name, &commit_sha)?;
    let primitive = resolve_primitive(&manifest)?;

    if scope == Scope::Project {
        if let Some(warning) = resolver::check_collision(name, project_root, Some(&clean_url))? {
            eprintln!("{warning}");
        }
    }

    install_primitive(install_root, scope, &primitive, &manifest, target_ids, Some(&SourceMetaInput {
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
    let manifest = load_manifest(&capability_dir)?;
    let primitive = resolve_primitive(&manifest)?;

    if scope == Scope::Project {
        if let Some(warning) = resolver::check_collision(&manifest.id, project_root, None)? {
            eprintln!("{warning}");
        }
    }

    install_primitive(install_root, scope, &primitive, &manifest, target_ids, None)
}

struct SourceMetaInput {
    source_type: String,
    url: String,
    source_ref: String,
    skill: String,
}

fn install_primitive(
    install_root: &Path,
    scope: Scope,
    primitive: &adapter::ResolvedPrimitive,
    manifest: &manifest::PrimitiveManifest,
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
        if !adapter.supports(&primitive.primitive) {
            return Err(CoralError::new(format!(
                "{} does not yet support {} primitives",
                adapter.display_name(),
                primitive.primitive
            )));
        }
        adapters.push(adapter);
    }

    let mut plans: Vec<(AdapterKind, Vec<adapter::PlannedFile>)> = Vec::new();
    for adapter in &adapters {
        let planned = adapter.plan(&primitive, install_root)?;
        plans.push((*adapter, planned));
    }

    let lockfile = lockfile::require_lockfile(install_root)?;
    for (adapter, planned_files) in &plans {
        let is_tracked = lockfile
            .primitives
            .get(&primitive.id)
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
                primitive.id,
                adapter.id(),
                lockfile::relative_or_absolute_fs(&target_path, install_root)
            );
        }

        let baseline_dir = install_root
            .join(".coral")
            .join("baselines")
            .join(adapter.id())
            .join(&primitive.id);
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
    if primitive.primitive == "tool" {
        if let Some(ref impl_cfg) = primitive.implementation {
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
                        format!(".agents/tools/{}/{}", primitive.id, impl_cfg.entrypoint)
                    }
                    AdapterKind::Claude => {
                        format!(".claude/tools/{}/{}", primitive.id, impl_cfg.entrypoint)
                    }
                };
                let mcp_args = vec![entrypoint_path];

                crate::adapters::mcp_register_tool(
                    install_root,
                    &mcp_path,
                    &primitive.id,
                    &mcp_command,
                    &mcp_args,
                )?;
            }
        }
    }

    let mut lockfile = lockfile;
    let existing_targets = lockfile
        .primitives
        .get(&primitive.id)
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

    lockfile.primitives.insert(
        primitive.id.clone(),
        lockfile::PrimitiveLockEntry {
            primitive: primitive.primitive.clone(),
            installed_version: primitive.version.clone(),
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

    let mut rows: Vec<(String, String, String, String, String, &'static str)> = Vec::new();

    if show_project {
        if let Ok(lockfile) = lockfile::require_lockfile(repo_root) {
            for (id, entry) in &lockfile.primitives {
                if let Some(kind) = kind_filter {
                    if entry.primitive != kind {
                        continue;
                    }
                }
                for (target_id, target_entry) in &entry.targets {
                    for emitted in &target_entry.emitted_files {
                        let status = lockfile::drift_status(repo_root, emitted);
                        rows.push((
                            id.clone(),
                            entry.installed_version.clone(),
                            "project".to_string(),
                            target_id.clone(),
                            emitted.path.clone(),
                            status,
                        ));
                    }
                }
            }
        }
    }

    if show_global {
        if let Some(home) = home_dir_opt() {
            let lock_path = home.join(".coral").join("coral-lock.json");
            if let Ok(lockfile) = lockfile::read_lockfile_at(&lock_path) {
                for (id, entry) in &lockfile.primitives {
                    if let Some(kind) = kind_filter {
                        if entry.primitive != kind {
                            continue;
                        }
                    }
                    for (target_id, target_entry) in &entry.targets {
                        for emitted in &target_entry.emitted_files {
                            let status = lockfile::drift_status(&home, emitted);
                            rows.push((
                                id.clone(),
                                entry.installed_version.clone(),
                                "global".to_string(),
                                target_id.clone(),
                                format!("~/{}", emitted.path),
                                status,
                            ));
                        }
                    }
                }
            }
        }
    }

    if rows.is_empty() {
        println!("no primitives installed");
        return Ok(());
    }

    rows.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)));

    for (id, version, scope, target, path, status) in &rows {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            id, version, scope, target, status, path
        );
    }
    Ok(())
}

// ── Status ──────────────────────────────────────────────────────────────────

pub fn cmd_status(repo_root: &Path) -> Result<()> {
    let mut found_any = false;

    if let Ok(lockfile) = lockfile::require_lockfile(repo_root) {
        for (id, entry) in &lockfile.primitives {
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
            found_any = true;
        }
    }

    if let Some(home) = home_dir_opt() {
        let lock_path = home.join(".coral").join("coral-lock.json");
        if let Ok(lockfile) = lockfile::read_lockfile_at(&lock_path) {
            for (id, entry) in &lockfile.primitives {
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
                        .map(|plf| plf.primitives.contains_key(id))
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
        println!("no primitives installed");
    }

    Ok(())
}

// ── Diff ────────────────────────────────────────────────────────────────────

pub fn cmd_diff(repo_root: &Path, capability_id: &str, target: Option<&str>) -> Result<()> {
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
    let entry = lockfile.primitives.remove(id).ok_or_else(|| {
        CoralError::new(format!("'{}' is not installed in {} scope", id, scope.as_str()))
    })?;

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

pub fn cmd_update(repo_root: &Path, id: &str, scope_str: Option<&str>) -> Result<()> {
    let (scope, entry, scope_root) = if let Some(s) = scope_str {
        let scope = resolver::Scope::from_str(s)
            .ok_or_else(|| CoralError::new(format!("invalid scope '{}'", s)))?;
        let root = match scope {
            Scope::Project => repo_root.to_path_buf(),
            Scope::Global => home_dir()?,
        };
        let lf = lockfile::require_lockfile(&root)?;
        let entry = lf.primitives.get(id).ok_or_else(|| {
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
                return Err(CoralError::new(format!(
                    "'{}' is not installed",
                    id
                )));
            }
        }
    };

    let source = entry.source.as_ref().ok_or_else(|| {
        CoralError::new(format!(
            "'{}' is not a git-sourced primitive — update only works for git sources",
            id
        ))
    })?;

    let (cache_dir, _clean_url) = git::clone_or_fetch(&source.url)?;
    let latest_sha = git::resolve_ref(&cache_dir)?;

    if latest_sha == entry.installed_version {
        println!("'{}' is already up to date", id);
        return Ok(());
    }

    let skill_dir = git::discover_skill(&cache_dir, &source.skill)?;
    let manifest = manifest::synthetic_manifest(&skill_dir, id, &latest_sha)?;
    let primitive = resolve_primitive(&manifest)?;

    let target_ids: Vec<String> = entry.targets.keys().cloned().collect();

    install_primitive(
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
    for (primitive_id, entry) in lockfile.primitives.iter() {
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

    for entry in lockfile.primitives.values_mut() {
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
