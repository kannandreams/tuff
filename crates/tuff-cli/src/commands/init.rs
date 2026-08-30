use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::adapter;
use crate::config;
use crate::display;
use crate::error::Result;
use crate::lockfile;
use crate::manifest::CapabilityType;

use super::home_dir;

const TUFF_GUIDE_CONTENT: &str = include_str!("../../assets/tuff-cli-guide.md");

pub fn cmd_init(repo_root: &Path, global: bool) -> Result<()> {
    if global {
        display::print_init_banner();
        let home = home_dir()?;
        let lock_path = crate::paths::global_lockfile(&home);
        lockfile::init_lockfile_at(&lock_path)?;
        let _ = config::read_global_config(&home)?;
        println!("initialized global Tuff state");
    } else {
        display::print_init_banner();
        let lock_path = lockfile::init_lockfile(repo_root)?;
        let had_config = crate::paths::project_config(repo_root).exists();
        let mut config = config::read_config(repo_root)?;
        if !config.agents.iter().any(|agent| agent == "open-agents") {
            config.agents.push("open-agents".to_string());
        }

        // Project adapter registration is reconstructed from tuff.lock so a
        // clone remains usable even when project preferences are absent.
        let lock = lockfile::read_lockfile_at(&lock_path)?;
        let targets: BTreeSet<String> = lock
            .capabilities
            .values()
            .flat_map(|entry| entry.targets.keys().cloned())
            .collect();
        for target in &targets {
            if !config.agents.iter().any(|agent| agent == target) {
                config.agents.push(target.clone());
            }
        }
        if !had_config && !targets.is_empty() && !targets.contains(&config.default_agent) {
            config.default_agent = targets
                .iter()
                .next()
                .cloned()
                .unwrap_or_else(|| config.default_agent.clone());
        }
        config::write_config(repo_root, &config)?;

        for dir in &["skills", "tools", "hooks", "workflows"] {
            let path = repo_root.join(".agents").join(dir);
            if !path.exists() {
                std::fs::create_dir_all(&path)?;
            }
        }

        println!(
            "initialized {}",
            lockfile::relative_or_absolute_fs(&lock_path, repo_root)
        );
        println!("scaffolded .agents/ — place your capabilities here:");

        let guide_path = repo_root
            .join(".agents")
            .join("skills")
            .join("tuff-cli-guide");
        if !guide_path.exists() {
            std::fs::create_dir_all(&guide_path)?;
            std::fs::write(guide_path.join("SKILL.md"), TUFF_GUIDE_CONTENT)?;

            let hash = lockfile::hash_bytes(TUFF_GUIDE_CONTENT.as_bytes());
            let baseline_tree_hash = crate::cache::hash_tree(&guide_path)?;
            crate::cache::populate(&home_dir()?, &baseline_tree_hash, &guide_path)?;
            let baseline_hash =
                lockfile::write_baseline_object(repo_root, TUFF_GUIDE_CONTENT.as_bytes())?;
            let mut lf = lockfile::require_lockfile(repo_root).unwrap_or(lockfile::Lockfile {
                version: lockfile::LOCKFILE_VERSION,
                capabilities: BTreeMap::new(),
            });

            let mut targets = BTreeMap::new();
            targets.insert(
                "open-agents".to_string(),
                lockfile::TargetLockEntry {
                    emitted_files: vec![adapter::EmittedFile {
                        path: ".agents/skills/tuff-cli-guide/SKILL.md".into(),
                        hash,
                        baseline_hash,
                    }],
                    managed_hooks: Vec::new(),
                    managed_mcp_entry: None,
                    ownership: lockfile::TargetOwnership::Generated,
                    sha256: baseline_tree_hash,
                    installed_path: ".agents/skills/tuff-cli-guide".into(),
                },
            );

            lf.capabilities.insert(
                "tuff-cli-guide".into(),
                lockfile::CapabilityLockEntry {
                    capability_type: CapabilityType::Skill,
                    installed_version: "0.1.0".into(),
                    description: "Guide for using Tuff CLI commands inside this repository.".into(),
                    source_path: String::new(),
                    targets,
                    source: None,
                    scope: "project".into(),
                    pack: None,
                    implementation: None,
                    parameters: None,
                    workflow: None,
                    server: None,
                },
            );

            lockfile::write_lockfile(repo_root, &lf)?;
        }
    }
    Ok(())
}
