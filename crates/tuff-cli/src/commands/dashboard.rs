//! `tuff dashboard` (RFC-108): reports about this project for a dashboard
//! server, and the server itself.

use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use tuff_server::{ServeConfig, Store, default_data_dir};

use crate::error::{Result, TuffError};

use super::{home_dir, render_table};

/// Where the dashboard keeps its database: `--data`, or the default under
/// `$XDG_DATA_HOME`.
fn data_dir(data: Option<&Path>) -> Result<PathBuf> {
    match data {
        Some(path) => Ok(path.to_path_buf()),
        None => Ok(default_data_dir(&home_dir()?)),
    }
}

/// `tuff dashboard serve`: listen until interrupted.
pub fn cmd_dashboard_serve(addr: SocketAddr, data: Option<&Path>, public_read: bool) -> Result<()> {
    let data_dir = data_dir(data)?;
    let shown_dir = data_dir.clone();
    tuff_server::run(
        ServeConfig {
            data_dir,
            addr,
            public_read,
        },
        move |bound| {
            println!("Dashboard listening on http://{bound}");
            println!("Data: {}", shown_dir.display());
            let _ = std::io::stdout().flush();
        },
    )
}

/// `tuff dashboard token create`: print the secret once.
pub fn cmd_dashboard_token_create(name: &str, data: Option<&Path>) -> Result<()> {
    let store = Store::open(&data_dir(data)?)?;
    let secret = store.create_token(name)?;
    println!("Created token '{name}'. It is shown once and cannot be shown again.");
    println!();
    println!("{secret}");
    println!();
    println!("Publish with TUFF_DASHBOARD_TOKEN set to it, or with --token.");
    Ok(())
}

/// `tuff dashboard token list`.
pub fn cmd_dashboard_token_list(data: Option<&Path>, json: bool) -> Result<()> {
    let store = Store::open(&data_dir(data)?)?;
    let tokens = store.tokens()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&tokens)?);
        return Ok(());
    }
    if tokens.is_empty() {
        println!("No tokens. Create one with 'tuff dashboard token create <name>'.");
        return Ok(());
    }
    let rows: Vec<Vec<String>> = tokens
        .into_iter()
        .map(|token| {
            vec![
                token.name,
                token.created_at,
                token.last_used_at.unwrap_or_else(|| "never".to_string()),
            ]
        })
        .collect();
    print!("{}", render_table(&["NAME", "CREATED", "LAST USED"], &rows));
    Ok(())
}

/// `tuff dashboard token revoke`.
pub fn cmd_dashboard_token_revoke(name: &str, data: Option<&Path>) -> Result<()> {
    Store::open(&data_dir(data)?)?.revoke_token(name)?;
    println!("Revoked token '{name}'.");
    Ok(())
}

pub struct PublishOptions<'a> {
    pub all: bool,
    pub outdated: bool,
    pub project: Option<&'a str>,
    pub dry_run: bool,
}

pub fn cmd_dashboard_publish(repo_root: &Path, options: PublishOptions<'_>) -> Result<()> {
    if !options.dry_run {
        return Err(TuffError::unsupported(
            "sending reports to a dashboard server is not built yet",
        )
        .with_hint("pass --dry-run to print the report instead"));
    }
    let projects = if options.all {
        let found = tuff_core::report::find_projects(repo_root)?;
        if found.is_empty() {
            return Err(TuffError::not_found(format!(
                "no tuff.lock under {}",
                repo_root.display()
            ))
            .with_hint("run 'tuff init' in each project folder first"));
        }
        found
    } else {
        vec![repo_root.to_path_buf()]
    };

    let mut reports = Vec::new();
    for project in &projects {
        let outdated = if options.outdated {
            Some(super::outdated::project_outdated_json(project)?)
        } else {
            None
        };
        reports.push(tuff_core::report::build_report(
            project,
            options.project,
            outdated,
        )?);
    }

    let output = if options.all {
        serde_json::to_string_pretty(&reports)?
    } else {
        serde_json::to_string_pretty(&reports[0])?
    };
    println!("{output}");
    Ok(())
}
