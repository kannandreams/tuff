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
};

pub fn cmd_init(repo_root: &Path) -> Result<()> {
    let lock_path = lockfile::init_lockfile(repo_root)?;
    let _ = config::read_config(repo_root)?;
    println!(
        "initialized {}",
        lockfile::relative_or_absolute_fs(&lock_path, repo_root)
    );
    Ok(())
}

pub fn cmd_add(
    repo_root: &Path,
    capability: &Path,
    target_ids: &[String],
    skill_name: Option<&str>,
) -> Result<()> {
    if git::is_git_url(&capability.to_string_lossy()) {
        return cmd_add_git(repo_root, &capability.to_string_lossy(), target_ids, skill_name);
    }
    cmd_add_local(repo_root, capability, target_ids)
}

fn cmd_add_git(
    repo_root: &Path,
    url: &str,
    target_ids: &[String],
    skill_name: Option<&str>,
) -> Result<()> {
    let skill_name = skill_name.ok_or_else(|| {
        CoralError::new("--skill is required when installing from a git URL")
    })?;

    let (cache_dir, clean_url) = git::clone_or_fetch(url)?;
    let commit_sha = git::resolve_ref(&cache_dir)?;
    let skill_dir = git::discover_skill(&cache_dir, skill_name)?;

    let manifest = manifest::synthetic_manifest(&skill_dir, skill_name, &commit_sha)?;
    let primitive = resolve_primitive(&manifest)?;

    install_primitive(repo_root, &primitive, &manifest, target_ids, Some(&SourceMetaInput {
        source_type: "git".to_string(),
        url: clean_url,
        source_ref: commit_sha.clone(),
        skill: skill_name.to_string(),
    }))
}

fn cmd_add_local(
    repo_root: &Path,
    capability: &Path,
    target_ids: &[String],
) -> Result<()> {
    let capability_dir = lockfile::absolutize(repo_root, capability);
    let manifest = load_manifest(&capability_dir)?;
    let primitive = resolve_primitive(&manifest)?;

    install_primitive(repo_root, &primitive, &manifest, target_ids, None)
}

struct SourceMetaInput {
    source_type: String,
    url: String,
    source_ref: String,
    skill: String,
}

fn install_primitive(
    repo_root: &Path,
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
        let planned = adapter.plan(&primitive, repo_root)?;
        plans.push((*adapter, planned));
    }

    let lockfile = lockfile::require_lockfile(repo_root)?;
    for (adapter, planned_files) in &plans {
        let is_tracked = lockfile
            .primitives
            .get(&primitive.id)
            .and_then(|e| e.targets.get(adapter.id()))
            .is_some();
        if !is_tracked {
            for f in planned_files {
                let target_path = repo_root.join(&f.path);
                if target_path.exists() {
                    return Err(CoralError::new(format!(
                        "refusing to overwrite untracked file at {}; remove it or track it in Coral first",
                        lockfile::relative_or_absolute_fs(&target_path, repo_root)
                    )));
                }
            }
        }
    }

    let mut new_targets: BTreeMap<String, TargetLockEntry> = BTreeMap::new();

    for (adapter, planned_files) in &plans {
        let mut emitted = Vec::new();

        for planned in planned_files {
            let target_path = repo_root.join(&planned.path);
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
                lockfile::relative_or_absolute_fs(&target_path, repo_root)
            );
        }

        let baseline_dir = repo_root
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

        let baseline_rel = lockfile::relative_or_absolute_fs(&baseline_dir, repo_root);
        new_targets.insert(
            adapter.id().to_string(),
            TargetLockEntry {
                baseline_dir: baseline_rel,
                emitted_files: emitted,
            },
        );
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
        lockfile::relative_or_absolute_fs(&manifest.root, repo_root)
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
        },
    );

    lockfile::write_lockfile(repo_root, &lockfile)?;
    Ok(())
}

pub fn cmd_list(repo_root: &Path) -> Result<()> {
    let lockfile = lockfile::require_lockfile(repo_root)?;
    if lockfile.primitives.is_empty() {
        println!("no primitives installed");
        return Ok(());
    }

    for (id, entry) in &lockfile.primitives {
        for (target_id, target_entry) in &entry.targets {
            for emitted in &target_entry.emitted_files {
                let status = lockfile::drift_status(repo_root, emitted);
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    id, entry.installed_version, target_id, status, emitted.path
                );
            }
        }
    }
    Ok(())
}

pub fn cmd_diff(repo_root: &Path, capability_id: &str, target: Option<&str>) -> Result<()> {
    let lockfile = lockfile::require_lockfile(repo_root)?;
    let entry = lockfile
        .primitives
        .get(capability_id)
        .ok_or_else(|| CoralError::new(format!("capability is not installed: {capability_id}")))?;

    let mut output = String::new();

    if let Some(tid) = target {
        output.push_str(&lockfile::diff_against_baseline(
            repo_root,
            capability_id,
            tid,
            entry,
        )?);
    } else {
        let mut first = true;
        for tid in entry.targets.keys() {
            let diff = lockfile::diff_against_baseline(repo_root, capability_id, tid, entry)?;
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

    let use_color = std::env::var_os("NO_COLOR").is_none();
    let dim = if use_color { "\x1b[2m" } else { "" };
    let reset = if use_color { "\x1b[0m" } else { "" };

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
        println!(
            "\n  {dim}*  = registered (use 'coral target add <id>' to register){reset}",
        );
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
