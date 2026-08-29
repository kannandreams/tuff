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

/// Classify one capability against its upstream.
///
/// A capability with no recorded source — anything installed from a pack, or
/// from a local path — cannot be checked at all. That is reported as
/// `not checked` rather than folded into `up to date`: an honest unknown sends
/// someone to look, a confident "up to date" does not.
fn classify(entry: &lockfile::CapabilityLockEntry) -> (String, String, String) {
    let Some(src) = &entry.source else {
        return (
            entry.installed_version.clone(),
            "—".to_string(),
            "not checked".to_string(),
        );
    };

    match git::clone_to_temp(&src.url, None).and_then(|(_guard, d, _)| git::resolve_ref(&d)) {
        Err(_) => (
            short_sha(&entry.installed_version).to_string(),
            "unavailable".to_string(),
            "error".to_string(),
        ),
        Ok(latest_sha) => {
            let status = if latest_sha == entry.installed_version {
                "up to date"
            } else {
                "outdated"
            };
            (
                short_sha(&entry.installed_version).to_string(),
                short_sha(&latest_sha).to_string(),
                status.to_string(),
            )
        }
    }
}

fn collect_rows(lf: &lockfile::Lockfile, rows: &mut Vec<OutdatedRow>) {
    for (id, entry) in &lf.capabilities {
        for target_id in entry.targets.keys() {
            let (current, latest, status) = classify(entry);
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

pub fn cmd_outdated(repo_root: &Path) -> Result<()> {
    let mut rows: Vec<OutdatedRow> = Vec::new();

    if let Ok(lf) = lockfile::require_lockfile(repo_root) {
        collect_rows(&lf, &mut rows);
    }

    if let Some(home) = home_dir_opt() {
        let lock_path = crate::paths::global_lockfile(&home);
        if let Ok(lf) = lockfile::read_lockfile_at(&lock_path) {
            collect_rows(&lf, &mut rows);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn entry(source: Option<lockfile::SourceMetadata>) -> lockfile::CapabilityLockEntry {
        lockfile::CapabilityLockEntry {
            capability_type: CapabilityType::Skill,
            installed_version: "1.0.0".to_string(),
            description: String::new(),
            source_path: "agent-capabilities/example".to_string(),
            targets: BTreeMap::new(),
            source,
            scope: "project".to_string(),
            pack: None,
        }
    }

    #[test]
    fn a_capability_with_no_source_is_reported_as_unchecked() {
        // Anything installed from a pack or a local path has no git source, so
        // there is nothing to compare against. Reporting "up to date" here
        // states a conclusion that was never reached: the row would claim the
        // capability is current while LATEST is unknown.
        let (current, latest, status) = classify(&entry(None));

        assert_eq!(status, "not checked");
        assert_ne!(
            status, "up to date",
            "an unchecked capability must not read as current"
        );
        assert_eq!(latest, "—");
        assert_eq!(current, "1.0.0");
    }
}
