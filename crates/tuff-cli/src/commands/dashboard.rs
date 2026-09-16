//! `tuff dashboard` (RFC-108): reports about this project for a dashboard
//! server.

use std::path::Path;

use crate::error::{Result, TuffError};

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
