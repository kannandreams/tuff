mod adapter;
mod adapters;
mod check;
mod commands;
mod config;
mod diff;
mod display;
mod error;
mod git;
mod lockfile;
mod manifest;
mod resolver;
mod tool;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use commands::{
    cmd_add, cmd_check, cmd_diff, cmd_init, cmd_list, cmd_outdated, cmd_remove, cmd_status,
    cmd_target_add, cmd_target_list, cmd_target_remove, cmd_update,
};
use error::Result;

#[derive(Parser)]
#[command(name = "coral", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize .coral state.
    Init {
        /// Initialize global scope (~/.coral/).
        #[arg(short = 'g', long = "global")]
        global: bool,
    },

    /// Install a capability.
    Add {
        /// Path to a capability directory or git URL.
        capability: PathBuf,

        /// Target harness to emit for (repeatable).
        #[arg(short = 't', long = "target", required = true)]
        target: Vec<String>,

        /// Skill name when installing from a git repository.
        #[arg(short = 's', long = "skill")]
        skill: Option<String>,

        /// Tool name when installing from a git repository.
        #[arg(long = "tool")]
        tool: Option<String>,

        /// Hook name when installing from a git repository.
        #[arg(long = "hook")]
        hook: Option<String>,

        /// Install to global scope (~/.coral/).
        #[arg(short = 'g', long = "global")]
        global: bool,
    },

    /// List installed capabilities.
    List {
        /// Filter by scope: project, global, or all.
        #[arg(short = 's', long = "scope", default_value = "all")]
        scope: String,

        /// Filter by capability type: skill, tool, hook.
        #[arg(short = 'p', long = "type")]
        kind: Option<String>,
    },

    /// Show detailed status for installed primitives.
    Status,

    /// Diff an installed capability against baseline.
    Diff {
        /// Installed capability id.
        capability_id: String,

        /// Target to diff (if not specified, diffs all targets).
        #[arg(short = 't', long = "target")]
        target: Option<String>,

        /// Diff against latest upstream source instead of baseline.
        #[arg(short = 'u', long = "upstream")]
        upstream: bool,
    },

    /// Remove an installed primitive.
    Remove {
        /// Primitive id to remove.
        id: String,

        /// Scope to remove from.
        #[arg(short = 's', long = "scope", default_value = "project")]
        scope: String,

        /// Target to remove from (if not specified, removes from all targets).
        #[arg(short = 't', long = "target")]
        target: Option<Vec<String>>,
    },

    /// Update an installed primitive from its source.
    Update {
        /// Primitive id to update.
        id: String,

        /// Scope to update.
        #[arg(short = 's', long = "scope")]
        scope: Option<String>,

        /// Dry run — show what would change without applying.
        #[arg(long = "check")]
        check: bool,

        /// Force overwrite local changes with upstream.
        #[arg(short = 'f', long = "force")]
        force: bool,
    },

    /// Validate installed capabilities (CI mode).
    Check {
        /// Output results as JSON.
        #[arg(long = "json")]
        json: bool,

        /// Report failures but exit with code 0.
        #[arg(long = "ignore-failures")]
        ignore_failures: bool,

        /// Validate global scope only.
        #[arg(long = "global")]
        global: bool,
    },

    /// Show installed capabilities with upstream update status.
    Outdated,

    /// Manage harness targets.
    Target {
        #[command(subcommand)]
        action: TargetCommand,
    },
}

#[derive(Subcommand)]
enum TargetCommand {
    /// List available and registered targets.
    List,

    /// Register a target for this repo.
    Add {
        /// Target adapter id.
        id: String,
    },

    /// Unregister a target and remove all emitted files.
    Remove {
        /// Target adapter id.
        id: String,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let repo_root = std::env::current_dir()?;

    match cli.command {
        None => Ok(display::print_welcome()),
        Some(Command::Init { global }) => cmd_init(&repo_root, global),
        Some(Command::Add {
            capability,
            target,
            skill,
            tool,
            hook,
            global,
        }) => cmd_add(
            &repo_root,
            &capability,
            &target,
            skill.as_deref(),
            tool.as_deref(),
            hook.as_deref(),
            global,
        ),
        Some(Command::List { scope, kind }) => cmd_list(&repo_root, &scope, kind.as_deref()),
        Some(Command::Status) => cmd_status(&repo_root),
        Some(Command::Diff {
            capability_id,
            target,
            upstream,
        }) => cmd_diff(&repo_root, &capability_id, target.as_deref(), upstream),
        Some(Command::Remove { id, scope, target }) => {
            cmd_remove(&repo_root, &id, &scope, target.as_deref())
        }
        Some(Command::Update {
            id,
            scope,
            check,
            force,
        }) => cmd_update(&repo_root, &id, scope.as_deref(), check, force),
        Some(Command::Check {
            json,
            ignore_failures,
            global,
        }) => cmd_check(&repo_root, json, ignore_failures, global),
        Some(Command::Outdated) => cmd_outdated(&repo_root),
        Some(Command::Target { action }) => match action {
            TargetCommand::List => cmd_target_list(&repo_root),
            TargetCommand::Add { id } => cmd_target_add(&repo_root, &id),
            TargetCommand::Remove { id } => cmd_target_remove(&repo_root, &id),
        },
    }
}
