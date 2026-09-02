use std::path::Path;

use crate::error::Result;
use crate::lockfile;
use crate::manifest::CapabilityType;
use crate::resolver;

use super::home_dir_opt;

pub fn cmd_status(repo_root: &Path) -> Result<()> {
    let mut found_any = false;

    if let Some(lf) = lockfile::read_optional_lockfile(&lockfile::project_lockfile(repo_root))? {
        for (id, entry) in &lf.capabilities {
            let mut flags = Vec::new();
            for target_entry in entry.targets.values() {
                if !target_entry.installed_path.is_empty() {
                    let path = repo_root.join(&target_entry.installed_path);
                    match crate::cache::hash_tree(&path) {
                        Ok(hash) if hash == target_entry.sha256 => {}
                        Ok(_) | Err(_) => flags.push("modified"),
                    }
                }
                for hook in &target_entry.managed_hooks {
                    let s = lockfile::managed_hook_status(repo_root, hook);
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

            if entry.capability_type == CapabilityType::Workflow && !entry.targets.is_empty() {
                let (_, target_entry) = entry.targets.iter().next().unwrap();
                if let Ok(content) = std::fs::read_to_string(
                    repo_root
                        .join(&target_entry.installed_path)
                        .join("workflow.toml"),
                ) {
                    let mut requires = Vec::new();
                    let mut current_id = String::new();
                    for line in content.lines() {
                        if line.starts_with("id = \"")
                            && !line.contains("version")
                            && !line.contains("description")
                            && !line.contains("type")
                        {
                            current_id = line
                                .trim_start_matches("id = \"")
                                .trim_end_matches('"')
                                .to_string();
                        }
                        if line.starts_with("type = \"") {
                            let raw_type =
                                line.trim_start_matches("type = \"").trim_end_matches('"');
                            if let Some(parsed) = CapabilityType::parse(raw_type)
                                && !current_id.is_empty()
                                && parsed != CapabilityType::Workflow
                            {
                                requires.push((current_id.clone(), parsed));
                                current_id.clear();
                            }
                        }
                    }

                    for (req_id, req_type) in &requires {
                        let child_status = if let Ok(lf) = lockfile::require_lockfile(repo_root) {
                            if let Some(child_entry) = lf.capabilities.get(req_id) {
                                let mut child_drift = Vec::new();
                                for ct_entry in child_entry.targets.values() {
                                    if ct_entry.installed_path.is_empty()
                                        || crate::cache::hash_tree(
                                            &repo_root.join(&ct_entry.installed_path),
                                        )
                                        .map(|hash| hash != ct_entry.sha256)
                                        .unwrap_or(true)
                                    {
                                        child_drift.push("modified");
                                    }
                                }
                                if child_drift.is_empty() {
                                    "clean".to_string()
                                } else {
                                    child_drift.join(",")
                                }
                            } else {
                                "not installed".to_string()
                            }
                        } else {
                            "unknown".to_string()
                        };
                        println!("  ├─ {req_id:<30} {req_type:<12} {child_status}");
                    }
                }
            }

            found_any = true;
        }
    }

    if let Some(home) = home_dir_opt() {
        let lock_path = crate::paths::global_lockfile(&home);
        if let Some(lf) = lockfile::read_optional_lockfile(&lock_path)? {
            for (id, entry) in &lf.capabilities {
                let mut flags = Vec::new();
                for target_entry in entry.targets.values() {
                    if !target_entry.installed_path.is_empty() {
                        let path = home.join(&target_entry.installed_path);
                        match crate::cache::hash_tree(&path) {
                            Ok(hash) if hash == target_entry.sha256 => {}
                            Ok(_) | Err(_) => flags.push("modified"),
                        }
                    }
                    for hook in &target_entry.managed_hooks {
                        let s = lockfile::managed_hook_status(&home, hook);
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
