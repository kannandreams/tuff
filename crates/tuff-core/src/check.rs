use std::path::Path;

use serde::Serialize;

use crate::error::Result;
use crate::lockfile;
use crate::manifest::CapabilityType;

#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub id: String,
    #[serde(rename = "type")]
    pub capability_type: CapabilityType,
    pub target: String,
    pub status: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
}

/// A policy rule recorded as not enforced for an agent (RFC-107 D6).
#[derive(Debug, Serialize)]
pub struct PolicyGap {
    pub id: String,
    pub target: String,
    /// One-based position of the rule in the policy.
    pub rule: usize,
    pub description: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct CheckOutcome {
    pub valid: bool,
    pub results: Vec<CheckResult>,
    /// Recorded policy rules an agent does not enforce. They do not affect
    /// `valid`; `tuff check --strict` fails on them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub gaps: Vec<PolicyGap>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckScope {
    ProjectAndGlobal,
    Global,
}

pub fn run_checks(repo_root: &Path, scope: CheckScope) -> Result<CheckOutcome> {
    let mut results = Vec::new();
    let mut gaps = Vec::new();

    if scope == CheckScope::ProjectAndGlobal
        && let Some(lf) = lockfile::read_optional_lockfile(&lockfile::project_lockfile(repo_root))?
    {
        check_lockfile(repo_root, &lf, &mut results, &mut gaps);
    }

    if let Some(home) = home_dir() {
        let lock_path = crate::paths::global_lockfile(&home);
        if let Some(lf) = lockfile::read_optional_lockfile(&lock_path)? {
            check_lockfile(&home, &lf, &mut results, &mut gaps);
        }
    }

    let valid = results.iter().all(|r| r.status == "ok");
    Ok(CheckOutcome {
        valid,
        results,
        gaps,
    })
}

fn check_lockfile(
    scope_root: &Path,
    lf: &lockfile::Lockfile,
    results: &mut Vec<CheckResult>,
    gaps: &mut Vec<PolicyGap>,
) {
    for (id, entry) in lf.capabilities.iter() {
        for (target_id, target_entry) in entry.targets.iter() {
            for unenforced in &target_entry.unenforced_rules {
                gaps.push(PolicyGap {
                    id: id.clone(),
                    target: target_id.clone(),
                    rule: unenforced.rule,
                    description: unenforced.description.clone(),
                    reason: unenforced.reason.clone(),
                });
            }

            let mut failing_files = Vec::new();

            if target_entry.installed_path.is_empty() {
                failing_files.push(id.clone());
            } else {
                let path = scope_root.join(&target_entry.installed_path);
                match crate::cache::hash_tree(&path) {
                    Ok(hash) if hash == target_entry.sha256 => {}
                    Ok(_) | Err(_) => failing_files.push(target_entry.installed_path.clone()),
                }
            }

            for hook in &target_entry.managed_hooks {
                if lockfile::managed_hook_status(scope_root, hook) != "clean" {
                    failing_files.push(format!("{}#{}", hook.settings_path, hook.event));
                }
            }

            for permission in &target_entry.managed_permissions {
                if crate::policy::managed_permission_status(scope_root, permission) != "clean" {
                    let location = crate::policy::permission_location(permission);
                    if !failing_files.contains(&location) {
                        failing_files.push(location);
                    }
                }
            }

            if let Some(managed_entry) = &target_entry.managed_mcp_entry
                && lockfile::managed_mcp_entry_status(scope_root, id, managed_entry) != "clean"
            {
                failing_files.push(format!("{}#{}", managed_entry.config_path, id));
            }

            let status = if failing_files.is_empty() {
                "ok"
            } else {
                "modified"
            };

            results.push(CheckResult {
                id: id.clone(),
                capability_type: entry.capability_type,
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
