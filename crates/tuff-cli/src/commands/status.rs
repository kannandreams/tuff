use std::path::Path;

use crate::error::Result;
use crate::lockfile;
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
