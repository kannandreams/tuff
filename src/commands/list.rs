use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::Path;

use tabled::Tabled;

use crate::error::Result;
use crate::lockfile::{self};
use crate::manifest::{CapabilityType, load_manifest};

use super::{home_dir_opt, render_table, short_sha};
use super::{style_capability_type, style_drift_status};

#[derive(Tabled)]
struct ListRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "TYPE")]
    capability_type: CapabilityType,
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

#[derive(Clone)]
pub(crate) struct InventoryRow {
    pub id: String,
    pub capability_type: CapabilityType,
    pub version: String,
    pub scope: String,
    pub target: String,
    pub status: String,
    pub path: String,
    pub description: String,
    pub source_type: String,
}

pub fn cmd_list(repo_root: &Path, scope_filter: &str, kind_filter: Option<&str>) -> Result<()> {
    let kind_type = kind_filter.and_then(CapabilityType::from_str);

    let show_project = scope_filter == "all" || scope_filter == "project";
    let show_global = scope_filter == "all" || scope_filter == "global";

    let mut inventory: Vec<InventoryRow> = Vec::new();

    if show_project && let Ok(lf) = lockfile::require_lockfile(repo_root) {
        inventory.extend(collect_lockfile_inventory(
            repo_root, &lf, "project", None, kind_type,
        ));
    }

    if show_global && let Some(home) = home_dir_opt() {
        let lock_path = crate::paths::global_lockfile(&home);
        if let Ok(lf) = lockfile::read_lockfile_at(&lock_path) {
            inventory.extend(collect_lockfile_inventory(
                &home,
                &lf,
                "global",
                Some("~/"),
                kind_type,
            ));
        }
    }

    if inventory.is_empty() {
        println!("no capabilities installed");
        return Ok(());
    }

    let mut rows = collapse_inventory_for_list(inventory);
    rows.sort_by(|a, b| a.scope.cmp(&b.scope).then_with(|| a.id.cmp(&b.id)));

    let table_rows: Vec<Vec<String>> = rows
        .into_iter()
        .map(|row| {
            vec![
                row.id,
                style_capability_type(row.capability_type),
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

pub(crate) fn collect_lockfile_inventory(
    root: &Path,
    lf: &lockfile::Lockfile,
    scope: &str,
    path_prefix: Option<&str>,
    kind_filter: Option<CapabilityType>,
) -> Vec<InventoryRow> {
    let mut rows = Vec::new();

    for (id, entry) in &lf.capabilities {
        if let Some(kind) = kind_filter
            && entry.capability_type != kind
        {
            continue;
        }

        let description = capability_description(root, entry);
        let source_type = capability_source_type(entry).to_string();

        for (target_id, target_entry) in &entry.targets {
            let managed_status = target_entry
                .managed_hooks
                .iter()
                .map(|hook| lockfile::managed_hook_status(root, hook))
                .find(|status| *status != "clean")
                .unwrap_or("clean");
            if target_entry.emitted_files.is_empty() {
                let installed_path = target_entry.installed_path.clone();
                let status = if installed_path.is_empty() {
                    "error"
                } else {
                    match crate::cache::hash_tree(&root.join(&installed_path)) {
                        Ok(hash) if hash == target_entry.sha256 => "clean",
                        Ok(_) | Err(_) => "modified",
                    }
                };
                rows.push(InventoryRow {
                    id: id.clone(),
                    capability_type: entry.capability_type,
                    version: short_sha(&entry.installed_version).to_string(),
                    scope: scope.to_string(),
                    target: target_id.clone(),
                    status: if managed_status == "clean" {
                        status.to_string()
                    } else {
                        managed_status.to_string()
                    },
                    path: match path_prefix {
                        Some(prefix) => format!("{prefix}{installed_path}"),
                        None => installed_path,
                    },
                    description: description.clone(),
                    source_type: source_type.clone(),
                });
                continue;
            }
            for emitted in &target_entry.emitted_files {
                let file_status = lockfile::drift_status(root, emitted);
                let status = if managed_status != "clean" {
                    managed_status
                } else {
                    file_status
                };
                let path = match path_prefix {
                    Some(prefix) => format!("{prefix}{}", emitted.path),
                    None => emitted.path.clone(),
                };
                rows.push(InventoryRow {
                    id: id.clone(),
                    capability_type: entry.capability_type,
                    version: short_sha(&entry.installed_version).to_string(),
                    scope: scope.to_string(),
                    target: target_id.clone(),
                    status: status.to_string(),
                    path,
                    description: description.clone(),
                    source_type: source_type.clone(),
                });
            }
        }
    }

    rows
}

fn capability_description(root: &Path, entry: &lockfile::CapabilityLockEntry) -> String {
    if !entry.description.trim().is_empty() {
        return entry.description.clone();
    }

    if entry.source_path.trim().is_empty() {
        return String::new();
    }

    let source_path = Path::new(&entry.source_path);
    let capability_dir = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        root.join(source_path)
    };

    load_manifest(&capability_dir)
        .map(|manifest| manifest.description)
        .unwrap_or_default()
}

fn capability_source_type(entry: &lockfile::CapabilityLockEntry) -> &'static str {
    if let Some(source) = &entry.source
        && source.source_type == "git"
    {
        return "git";
    }
    "local"
}

pub(crate) fn project_inventory(
    repo_root: &Path,
    kind_filter: Option<CapabilityType>,
) -> Result<Vec<InventoryRow>> {
    let lf = lockfile::require_lockfile(repo_root)?;
    Ok(collect_lockfile_inventory(
        repo_root,
        &lf,
        "project",
        None,
        kind_filter,
    ))
}

fn collapse_inventory_for_list(inventory: Vec<InventoryRow>) -> Vec<ListRow> {
    let mut grouped: BTreeMap<(String, String, String), Vec<InventoryRow>> = BTreeMap::new();
    for row in inventory {
        grouped
            .entry((row.scope.clone(), row.id.clone(), row.target.clone()))
            .or_default()
            .push(row);
    }

    grouped
        .into_values()
        .map(|mut rows| {
            rows.sort_by(|a, b| a.path.cmp(&b.path));
            let first = rows[0].clone();
            let status = aggregate_status(&rows);
            let path = display_path_for_rows(&rows);
            ListRow {
                id: first.id,
                capability_type: first.capability_type,
                version: first.version,
                scope: first.scope,
                target: first.target,
                status: style_drift_status(status),
                path,
            }
        })
        .collect()
}

fn aggregate_status(rows: &[InventoryRow]) -> &'static str {
    if rows.iter().any(|row| row.status == "missing") {
        "missing"
    } else if rows.iter().any(|row| row.status == "modified") {
        "modified"
    } else {
        "clean"
    }
}

fn display_path_for_rows(rows: &[InventoryRow]) -> String {
    let primary = rows
        .iter()
        .find(|row| Path::new(&row.path).file_name() == Some(OsStr::new("SKILL.md")))
        .unwrap_or(&rows[0]);

    if rows.len() > 1 {
        format!("{} (+{} files)", primary.path, rows.len() - 1)
    } else {
        primary.path.clone()
    }
}
