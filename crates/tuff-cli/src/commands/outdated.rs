use std::path::Path;

use crate::error::Result;
use crate::git;
use crate::lockfile;
use crate::manifest::CapabilityType;

use super::{home_dir_opt, render_table, short_sha, style_outdated_status};

struct OutdatedRow {
    id: String,
    capability_type: CapabilityType,
    target: String,
    current: String,
    latest: String,
    status: String,
}

pub fn cmd_outdated(repo_root: &Path) -> Result<()> {
    let mut rows: Vec<OutdatedRow> = Vec::new();

    if let Ok(lf) = lockfile::require_lockfile(repo_root) {
        for (id, entry) in &lf.capabilities {
            for target_id in entry.targets.keys() {
                let (current, latest, status) = if let Some(src) = &entry.source {
                    let latest_sha = match git::clone_to_temp(&src.url, None)
                        .and_then(|(_guard, d, _)| git::resolve_ref(&d))
                    {
                        Ok(sha) => sha,
                        Err(_) => {
                            rows.push(OutdatedRow {
                                id: id.clone(),
                                capability_type: entry.capability_type,
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
                    capability_type: entry.capability_type,
                    target: target_id.clone(),
                    current,
                    latest,
                    status,
                });
            }
        }
    }

    if let Some(home) = home_dir_opt() {
        let lock_path = crate::paths::global_lockfile(&home);
        if let Ok(lf) = lockfile::read_lockfile_at(&lock_path) {
            for (id, entry) in &lf.capabilities {
                for target_id in entry.targets.keys() {
                    let (current, latest, status) = if let Some(src) = &entry.source {
                        let latest_sha = match git::clone_to_temp(&src.url, None)
                            .and_then(|(_guard, d, _)| git::resolve_ref(&d))
                        {
                            Ok(sha) => sha,
                            Err(_) => {
                                rows.push(OutdatedRow {
                                    id: id.clone(),
                                    capability_type: entry.capability_type,
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
                        capability_type: entry.capability_type,
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

    let table_rows: Vec<Vec<String>> = rows
        .into_iter()
        .map(|r| {
            let status = style_outdated_status(&r.status);
            let row = OutdatedRow { status, ..r };
            vec![
                row.id,
                row.capability_type.to_string(),
                row.target,
                row.current,
                row.latest,
                row.status,
            ]
        })
        .collect();

    println!(
        "{}",
        render_table(
            &["ID", "TYPE", "AGENT", "CURRENT", "LATEST", "STATUS"],
            &table_rows
        )
    );
    Ok(())
}
