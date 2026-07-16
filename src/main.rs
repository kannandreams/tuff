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
    cmd_add, cmd_agent_add, cmd_agent_list, cmd_agent_remove, cmd_check, cmd_create, cmd_delete,
    cmd_diff, cmd_init, cmd_list, cmd_outdated, cmd_status, cmd_untrack, cmd_update,
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

        /// Agent harness to emit for (repeatable).
        #[arg(short = 'a', long = "agent", required = true)]
        agent: Vec<String>,

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

        /// Agent to diff (if not specified, diffs all agents).
        #[arg(short = 'a', long = "agent")]
        agent: Option<String>,

        /// Diff against latest upstream source instead of baseline.
        #[arg(short = 'u', long = "upstream")]
        upstream: bool,
    },

    /// Delete Coral-generated capability files.
    Delete {
        /// Capability id to delete.
        id: String,

        /// Scope to delete from.
        #[arg(short = 's', long = "scope", default_value = "project")]
        scope: String,

        /// Agent to delete from (repeatable).
        #[arg(short = 'a', long = "agent", required = true)]
        agent: Vec<String>,

        /// Delete files even when they have local modifications.
        #[arg(short = 'f', long = "force")]
        force: bool,
    },

    /// Stop tracking a capability without deleting its agent files.
    Untrack {
        /// Capability id to untrack.
        id: String,

        /// Scope to untrack from.
        #[arg(short = 's', long = "scope", default_value = "project")]
        scope: String,

        /// Agent to untrack (repeatable).
        #[arg(short = 'a', long = "agent", required = true)]
        agent: Vec<String>,
    },

    /// Reconcile an installed capability with its source or accept local edits.
    Update {
        /// Capability id to update.
        id: String,

        /// Scope to update.
        #[arg(short = 's', long = "scope")]
        scope: Option<String>,

        /// Dry run — show what would change without applying.
        #[arg(long = "check")]
        check: bool,

        /// Agent harness to update (repeatable; defaults to all recorded agents).
        #[arg(short = 'a', long = "agent")]
        agent: Vec<String>,

        /// Force overwrite local changes with upstream (Git sources only).
        #[arg(short = 'f', long = "force")]
        force: bool,
    },

    /// Create and track a new capability.
    Create {
        #[command(subcommand)]
        kind: CreateCommand,
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

    /// Manage agent harnesses.
    Agent {
        #[command(subcommand)]
        action: AgentCommand,
    },
}

#[derive(Subcommand)]
enum CreateCommand {
    /// Create and track a skill.
    Skill {
        /// Capability id.
        id: String,
        /// Agent harnesses to scaffold for (repeatable).
        #[arg(short = 'a', long = "agent", default_values = ["open-agents"])]
        agent: Vec<String>,
    },
    /// Create and track a tool.
    Tool {
        /// Capability id.
        id: String,
        /// Agent harnesses to scaffold for (repeatable).
        #[arg(short = 'a', long = "agent", default_values = ["open-agents"])]
        agent: Vec<String>,
    },
    /// Create and track a hook.
    Hook {
        /// Capability id.
        id: String,
        /// Agent harnesses to scaffold for (repeatable).
        #[arg(short = 'a', long = "agent", default_values = ["open-agents"])]
        agent: Vec<String>,
    },
    /// Create and track a workflow.
    Workflow {
        /// Capability id.
        id: String,
        /// Agent harnesses to scaffold for (repeatable).
        #[arg(short = 'a', long = "agent", default_values = ["open-agents"])]
        agent: Vec<String>,
    },
}

#[derive(Subcommand)]
enum AgentCommand {
    /// List available and registered agents.
    List,

    /// Register an agent for this repo.
    Add {
        /// Agent adapter id.
        id: String,
    },

    /// Unregister an agent without changing installed capabilities.
    Remove {
        /// Agent adapter id.
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
        Some(Command::Create { kind }) => match kind {
            CreateCommand::Skill { id, agent } => cmd_create(&repo_root, "skill", &id, &agent),
            CreateCommand::Tool { id, agent } => cmd_create(&repo_root, "tool", &id, &agent),
            CreateCommand::Hook { id, agent } => cmd_create(&repo_root, "hook", &id, &agent),
            CreateCommand::Workflow { id, agent } => {
                cmd_create(&repo_root, "workflow", &id, &agent)
            }
        },
        Some(Command::Add {
            capability,
            agent,
            skill,
            tool,
            hook,
            global,
        }) => cmd_add(
            &repo_root,
            &capability,
            &agent,
            skill.as_deref(),
            tool.as_deref(),
            hook.as_deref(),
            global,
        ),
        Some(Command::List { scope, kind }) => cmd_list(&repo_root, &scope, kind.as_deref()),
        Some(Command::Status) => cmd_status(&repo_root),
        Some(Command::Diff {
            capability_id,
            agent,
            upstream,
        }) => cmd_diff(&repo_root, &capability_id, agent.as_deref(), upstream),
        Some(Command::Delete {
            id,
            scope,
            agent,
            force,
        }) => cmd_delete(&repo_root, &id, &scope, &agent, force),
        Some(Command::Untrack { id, scope, agent }) => cmd_untrack(&repo_root, &id, &scope, &agent),
        Some(Command::Update {
            id,
            scope,
            check,
            agent,
            force,
        }) => cmd_update(&repo_root, &id, scope.as_deref(), &agent, check, force),
        Some(Command::Check {
            json,
            ignore_failures,
            global,
        }) => cmd_check(&repo_root, json, ignore_failures, global),
        Some(Command::Outdated) => cmd_outdated(&repo_root),
        Some(Command::Agent { action }) => match action {
            AgentCommand::List => cmd_agent_list(&repo_root),
            AgentCommand::Add { id } => cmd_agent_add(&repo_root, &id),
            AgentCommand::Remove { id } => cmd_agent_remove(&repo_root, &id),
        },
    }
}
