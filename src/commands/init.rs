use std::collections::BTreeMap;
use std::path::Path;

use crate::adapter;
use crate::config;
use crate::display;
use crate::error::Result;
use crate::lockfile;
use crate::manifest::CapabilityType;

use super::home_dir;

const CORAL_GUIDE_CONTENT: &str = include_str!("../../assets/coral-cli-guide.md");

pub fn cmd_init(repo_root: &Path, global: bool) -> Result<()> {
    if global {
        display::print_init_banner();
        let home = home_dir()?;
        let lock_path = home.join(".coral").join("coral-lock.json");
        lockfile::init_lockfile_at(&lock_path)?;
        let _ = config::read_config(&home)?;
        println!("initialized ~/.coral/coral-lock.json");
    } else {
        display::print_init_banner();
        let lock_path = lockfile::init_lockfile(repo_root)?;
        let mut config = config::read_config(repo_root)?;
        if !config.agents.iter().any(|agent| agent == "open-agents") {
            config.agents.push("open-agents".to_string());
            config::write_config(repo_root, &config)?;
        }

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
            .join("coral-cli-guide");
        if !guide_path.exists() {
            std::fs::create_dir_all(&guide_path)?;
            std::fs::write(guide_path.join("SKILL.md"), CORAL_GUIDE_CONTENT)?;

            let hash = lockfile::hash_bytes(CORAL_GUIDE_CONTENT.as_bytes());
            let baseline_hash =
                lockfile::write_baseline_object(repo_root, CORAL_GUIDE_CONTENT.as_bytes())?;
            let mut lf = lockfile::require_lockfile(repo_root).unwrap_or(lockfile::Lockfile {
                version: lockfile::LOCKFILE_VERSION,
                capabilities: BTreeMap::new(),
            });

            let mut targets = BTreeMap::new();
            targets.insert(
                "open-agents".to_string(),
                lockfile::TargetLockEntry {
                    emitted_files: vec![adapter::EmittedFile {
                        path: ".agents/skills/coral-cli-guide/SKILL.md".into(),
                        hash,
                        baseline_hash,
                    }],
                    managed_hooks: Vec::new(),
                    ownership: lockfile::TargetOwnership::Generated,
                },
            );

            lf.capabilities.insert(
                "coral-cli-guide".into(),
                lockfile::CapabilityLockEntry {
                    capability_type: CapabilityType::Skill,
                    installed_version: "0.1.0".into(),
                    description: "Guide for using Coral CLI commands inside this repository."
                        .into(),
                    source_path: String::new(),
                    targets,
                    source: None,
                    scope: "project".into(),
                },
            );

            lockfile::write_lockfile(repo_root, &lf)?;
        }
    }
    Ok(())
}
