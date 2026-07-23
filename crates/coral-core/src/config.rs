use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::paths;

pub const DEFAULT_AGENT: &str = "open-agents";

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CoralConfig {
    #[serde(rename = "agents", alias = "targets")]
    pub agents: Vec<String>,
    #[serde(rename = "defaultAgent", alias = "default_agent")]
    pub default_agent: String,
}

impl Default for CoralConfig {
    fn default() -> Self {
        Self {
            agents: vec![],
            default_agent: DEFAULT_AGENT.to_string(),
        }
    }
}

pub fn read_config(repo_root: &Path) -> Result<CoralConfig> {
    read_config_at(&paths::project_config(repo_root))
}

pub fn read_global_config(home: &Path) -> Result<CoralConfig> {
    read_config_at(&paths::user_config(home))
}

fn read_config_at(path: &Path) -> Result<CoralConfig> {
    if path.exists() {
        let config: CoralConfig = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        Ok(config)
    } else {
        let default = CoralConfig::default();
        write_config_at(path, &default)?;
        Ok(default)
    }
}

pub fn write_config(repo_root: &Path, config: &CoralConfig) -> Result<()> {
    write_config_at(&paths::project_config(repo_root), config)
}

pub fn write_global_config(home: &Path, config: &CoralConfig) -> Result<()> {
    write_config_at(&paths::user_config(home), config)
}

fn write_config_at(path: &Path, config: &CoralConfig) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(config)? + "\n")?;
    Ok(())
}
