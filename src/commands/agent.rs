use std::collections::HashSet;
use std::path::Path;

use crate::adapter::{AdapterKind, AgentAdapter};
use crate::config;
use crate::error::Result;

use super::{home_dir, render_table};

pub fn cmd_agent_list(repo_root: &Path, global: bool) -> Result<()> {
    let config_root = if global { home_dir()? } else { repo_root.to_path_buf() };
    let config = config::read_config(&config_root)?;
    let registered: HashSet<&str> = config.agents.iter().map(|s| s.as_str()).collect();

    let table_rows: Vec<Vec<String>> = AdapterKind::all()
        .iter()
        .map(|a| {
            vec![
                a.id().to_string(),
                a.supported_agents().join(", "),
                a.kinds_supported()
                    .iter()
                    .map(|ct| ct.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                if registered.contains(a.id()) { "yes".to_string() } else { String::new() },
                if config.default_agent == a.id() { "yes".to_string() } else { String::new() },
            ]
        })
        .collect();
    println!(
        "{}",
        render_table(
            &[
                "AGENT",
                "AGENTS SUPPORTED",
                "CAPABILITIES",
                "REGISTERED",
                "DEFAULT",
            ],
            &table_rows
        )
    );
    println!(
        "\n  REGISTERED = available for Coral operations in this {} config",
        if global { "global" } else { "project" }
    );
    println!(
        "  DEFAULT = used when --agent is omitted ({})",
        if global { "global" } else { "project" }
    );
    Ok(())
}

pub fn cmd_agent_add(repo_root: &Path, id: &str) -> Result<()> {
    let adapter = AdapterKind::from_id(id).ok_or_else(|| {
        crate::error::CoralError::new(format!(
            "unknown agent '{}'; use 'coral agent list' to see available agents",
            id
        ))
    })?;

    let mut config = config::read_config(repo_root)?;
    adapter.ensure_project_dir(repo_root)?;
    if config.agents.contains(&adapter.id().to_string()) {
        println!("agent '{}' is already registered", id);
        return Ok(());
    }

    config.agents.push(adapter.id().to_string());
    config::write_config(repo_root, &config)?;
    println!(
        "registered agent '{}' ({})",
        adapter.id(),
        adapter.display_name()
    );
    Ok(())
}

pub fn cmd_agent_remove(repo_root: &Path, id: &str) -> Result<()> {
    let adapter = AdapterKind::from_id(id).ok_or_else(|| {
        crate::error::CoralError::new(format!(
            "unknown agent '{}'; use 'coral agent list' to see available agents",
            id
        ))
    })?;

    let mut config = config::read_config(repo_root)?;
    let was_registered = config.agents.contains(&adapter.id().to_string());

    config.agents.retain(|t| t != adapter.id());
    config::write_config(repo_root, &config)?;

    if was_registered {
        println!("unregistered agent '{}'", adapter.id());
    } else {
        println!("removed agent '{}'", adapter.id());
    }
    Ok(())
}

pub fn cmd_agent_set_default(repo_root: &Path, id: &str, global: bool) -> Result<()> {
    let adapter = AdapterKind::from_id(id).ok_or_else(|| {
        crate::error::CoralError::new(format!(
            "unknown agent '{}'; use 'coral agent list' to see available agents",
            id
        ))
    })?;
    let config_root = if global { home_dir()? } else { repo_root.to_path_buf() };
    let mut config = config::read_config(&config_root)?;
    config.default_agent = adapter.id().to_string();
    config::write_config(&config_root, &config)?;
    println!(
        "set default agent '{}' ({})",
        adapter.id(),
        if global { "global" } else { "project" }
    );
    Ok(())
}
