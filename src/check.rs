use std::path::Path;

use serde::Serialize;

use crate::error::Result;
use crate::lockfile;

#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub id: String,
    #[serde(rename = "type")]
    pub capability_type: String,
    pub target: String,
    pub status: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CheckOutcome {
    pub valid: bool,
    pub results: Vec<CheckResult>,
}

pub fn run_checks(repo_root: &Path) -> Result<CheckOutcome> {
    let mut results = Vec::new();

    // Project scope
    if let Ok(lf) = lockfile::require_lockfile(repo_root) {
        check_lockfile(repo_root, &lf, &mut results);
    }

    // Global scope
    if let Some(home) = home_dir() {
        let lock_path = home.join(".coral").join("coral-lock.json");
        if let Ok(lf) = lockfile::read_lockfile_at(&lock_path) {
            check_lockfile(&home, &lf, &mut results);
        }
    }

    let valid = results.iter().all(|r| r.status == "ok");
    Ok(CheckOutcome { valid, results })
}

fn check_lockfile(scope_root: &Path, lf: &lockfile::Lockfile, results: &mut Vec<CheckResult>) {
    for (id, entry) in lf.capabilities.iter() {
        for (target_id, target_entry) in entry.targets.iter() {
            let mut failing_files = Vec::new();

            for emitted in &target_entry.emitted_files {
                let file_path = scope_root.join(&emitted.path);

                if !file_path.exists() {
                    failing_files.push(emitted.path.clone());
                    continue;
                }

                if let Ok(content) = std::fs::read(&file_path) {
                    let hash = lockfile::hash_bytes(&content);
                    if hash != emitted.hash {
                        failing_files.push(emitted.path.clone());
                    }
                }
            }

            let status = if failing_files.is_empty() {
                "ok"
            } else {
                "modified"
            };

            results.push(CheckResult {
                id: id.clone(),
                capability_type: entry.capability_type.clone(),
                target: target_id.clone(),
                status: status.to_string(),
                files: failing_files,
            });
        }
    }
}

fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var("HOME").ok().map(std::path::PathBuf::from)
}
