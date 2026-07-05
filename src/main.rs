mod adapter;
mod adapters;
mod commands;
mod config;
mod display;
mod error;
mod lockfile;
mod manifest;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use commands::{
    cmd_add, cmd_diff, cmd_init, cmd_list, cmd_target_add, cmd_target_list, cmd_target_remove,
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
    Init,

    /// Install a local capability.
    Add {
        /// Path to a capability directory.
        capability: PathBuf,

        /// Target harness to emit for (repeatable).
        #[arg(short = 't', long = "target", required = true)]
        target: Vec<String>,
    },

    /// List installed capabilities.
    List,

    /// Diff an installed capability against baseline.
    Diff {
        /// Installed capability id.
        capability_id: String,

        /// Target to diff (if not specified, diffs all targets).
        #[arg(short = 't', long = "target")]
        target: Option<String>,
    },

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
        Some(Command::Init) => cmd_init(&repo_root),
        Some(Command::Add { capability, target }) => cmd_add(&repo_root, &capability, &target),
        Some(Command::List) => cmd_list(&repo_root),
        Some(Command::Diff {
            capability_id,
            target,
        }) => cmd_diff(&repo_root, &capability_id, target.as_deref()),
        Some(Command::Target { action }) => match action {
            TargetCommand::List => cmd_target_list(&repo_root),
            TargetCommand::Add { id } => cmd_target_add(&repo_root, &id),
            TargetCommand::Remove { id } => cmd_target_remove(&repo_root, &id),
        },
    }
}
