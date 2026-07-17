use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;

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

fn config_path(repo_root: &Path) -> std::path::PathBuf {
    repo_root.join(".coral").join("config.json")
}

pub fn read_config(repo_root: &Path) -> Result<CoralConfig> {
    let path = config_path(repo_root);
    if path.exists() {
        let config: CoralConfig = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        Ok(config)
    } else {
        let default = CoralConfig::default();
        write_config(repo_root, &default)?;
        Ok(default)
    }
}

pub fn write_config(repo_root: &Path, config: &CoralConfig) -> Result<()> {
    let path = config_path(repo_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(config)? + "\n")?;
    Ok(())
}
