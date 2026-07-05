use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use tabled::{
    settings::{object::Columns, Modify, Style, Width},
    Table, Tabled,
};

use crate::{
    adapter::{self, AdapterKind, resolve_capability},
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

    let mut rows: Vec<(String, String, String, String, String, &'static str)> = Vec::new();

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
                for (id, entry) in &lockfile.capabilities {
                    if let Some(kind) = kind_filter {
                        if entry.capability_type != kind {
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
        println!("no capabilities installed");
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

// ── Outdated ────────────────────────────────────────────────────────────────

fn short_sha(v: &str) -> &str {
    if v.len() >= 7 && v.chars().all(|c| c.is_ascii_hexdigit()) {
        &v[..7]
    } else {
        v
    }
}

pub fn cmd_outdated(repo_root: &Path) -> Result<()> {
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

    let use_color = std::env::var_os("NO_COLOR").is_none();
    let green = if use_color { "\x1b[32m" } else { "" };
    let red = if use_color { "\x1b[31m" } else { "" };
    let reset = if use_color { "\x1b[0m" } else { "" };

    let styled_rows: Vec<OutdatedRow> = rows
        .into_iter()
        .map(|r| {
            let status = match r.status.as_str() {
                "up to date" => format!("  {green}●{reset} {}", r.status),
                "outdated" => format!("  {red}●{reset} {}", r.status),
                "error" => format!("  {red}●{reset} {}", r.status),
                _ => r.status,
            };
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
