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
    cmd_add, cmd_agent_add, cmd_agent_list, cmd_agent_remove, cmd_agent_set_default, cmd_check,
    cmd_create, cmd_delete, cmd_diff, cmd_generate_index, cmd_generate_report, cmd_init, cmd_list,
    cmd_outdated, cmd_status, cmd_untrack, cmd_update,
};
use error::{CoralError, Result};
use manifest::CapabilityType;

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

    /// Create and track a new capability.
    Create {
        #[command(subcommand)]
        kind: CreateCommand,
    },

    /// Install a capability.
    Add {
        /// Path or URL (auto-detect type). Use a subcommand to specify type explicitly.
        source: Option<PathBuf>,

        /// Override the capability name (only when type is auto-detected).
        #[arg(short = 'n', long = "name")]
        name: Option<String>,

        /// Agent harness to emit for (repeatable).
        #[arg(short = 'a', long = "agent")]
        agent: Vec<String>,

        /// Install to global scope (~/.coral/).
        #[arg(short = 'g', long = "global")]
        global: bool,

        #[command(subcommand)]
        kind: Option<AddCommand>,
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

    /// Generate derived Coral artifacts.
    Generate {
        #[command(subcommand)]
        artifact: GenerateCommand,
    },

    /// Show installed capabilities with upstream update status.
    Outdated,

    /// Diff an installed capability against baseline.
    Diff {
        /// Installed capability id.
        capability_id: String,

        /// Agent to diff (defaults to the configured agent).
        #[arg(short = 'a', long = "agent")]
        agent: Option<String>,

        /// Diff against latest upstream source instead of baseline.
        #[arg(short = 'u', long = "upstream")]
        upstream: bool,
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

        /// Agent harness to update (repeatable; defaults to the configured agent).
        #[arg(short = 'a', long = "agent")]
        agent: Vec<String>,

        /// Force overwrite local changes with upstream (Git sources only).
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

    /// Delete Coral-generated capability files.
    Delete {
        /// Capability id to delete.
        id: String,

        /// Scope to delete from.
        #[arg(short = 's', long = "scope", default_value = "project")]
        scope: String,

        /// Agent to delete from (repeatable).
        #[arg(short = 'a', long = "agent")]
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
        #[arg(short = 'a', long = "agent")]
        agent: Vec<String>,
    },

    /// Manage agent harnesses.
    Agent {
        #[command(subcommand)]
        action: AgentCommand,
    },
}

#[derive(Subcommand)]
enum GenerateCommand {
    /// Generate an agent-facing capability index.
    Index {
        /// Agent harness to generate an index for (defaults to the configured agent).
        #[arg(short = 'a', long = "agent")]
        agent: Option<String>,

        /// Output path. Defaults to the agent's standard CAPABILITIES.md path.
        #[arg(short = 'o', long = "output")]
        output: Option<PathBuf>,
    },

    /// Generate a project capability report.
    Report {
        /// Output path. Defaults to .coral/reports/coral-report.md.
        #[arg(short = 'o', long = "output")]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum CreateCommand {
    /// Create and track a skill.
    Skill {
        /// Capability id.
        id: String,
        /// Agent harnesses to scaffold for (repeatable).
        #[arg(short = 'a', long = "agent")]
        agent: Vec<String>,
    },
    /// Create and track a tool.
    Tool {
        /// Capability id.
        id: String,
        /// Agent harnesses to scaffold for (repeatable).
        #[arg(short = 'a', long = "agent")]
        agent: Vec<String>,
    },
    /// Create and track a hook.
    Hook {
        /// Capability id.
        id: String,
        /// Agent harnesses to scaffold for (repeatable).
        #[arg(short = 'a', long = "agent")]
        agent: Vec<String>,
    },
    /// Create and track a workflow.
    Workflow {
        /// Capability id.
        id: String,
        /// Agent harnesses to scaffold for (repeatable).
        #[arg(short = 'a', long = "agent")]
        agent: Vec<String>,
    },
}

#[derive(Subcommand)]
enum AddCommand {
    /// Install a skill from a local path or git URL.
    Skill {
        /// Path to capability directory, file, or git URL.
        source: PathBuf,
        /// Override the capability name (default: inferred from source).
        name: Option<String>,
        /// Agent harness to emit for (repeatable).
        #[arg(short = 'a', long = "agent")]
        agent: Vec<String>,
        /// Install to global scope (~/.coral/).
        #[arg(short = 'g', long = "global")]
        global: bool,
    },
    /// Install a tool from a local path or git URL.
    Tool {
        source: PathBuf,
        name: Option<String>,
        #[arg(short = 'a', long = "agent")]
        agent: Vec<String>,
        #[arg(short = 'g', long = "global")]
        global: bool,
    },
    /// Install a hook from a local path or git URL.
    Hook {
        source: PathBuf,
        name: Option<String>,
        /// Native harness hook fragment to merge, relative to the source directory.
        #[arg(long = "hook-file")]
        hook_file: Option<PathBuf>,
        #[arg(short = 'a', long = "agent")]
        agent: Vec<String>,
        #[arg(short = 'g', long = "global")]
        global: bool,
    },
    /// Install a workflow from a local path or git URL.
    Workflow {
        source: PathBuf,
        name: Option<String>,
        #[arg(short = 'a', long = "agent")]
        agent: Vec<String>,
        #[arg(short = 'g', long = "global")]
        global: bool,
    },
}

fn reject_parent_add_options(
    source: Option<&PathBuf>,
    name: Option<&String>,
    agent: &[String],
    global: bool,
) -> Result<()> {
    if source.is_some() || name.is_some() || !agent.is_empty() || global {
        return Err(CoralError::new(
            "for typed 'coral add' commands, put --agent and --global after the capability source",
        ));
    }
    Ok(())
}

#[derive(Subcommand)]
enum AgentCommand {
    /// List available and registered agents.
    List {
        /// Show the global agent configuration.
        #[arg(short = 'g', long = "global")]
        global: bool,
    },

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

    /// Set the default agent used when --agent is omitted.
    SetDefault {
        /// Agent adapter id.
        id: String,

        /// Set the default for global operations (~/.coral/).
        #[arg(short = 'g', long = "global")]
        global: bool,
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
        None => {
            display::print_welcome();
            Ok(())
        }
        Some(Command::Init { global }) => cmd_init(&repo_root, global),
        Some(Command::Create { kind }) => match kind {
            CreateCommand::Skill { id, agent } => {
                cmd_create(&repo_root, CapabilityType::Skill, &id, &agent)
            }
            CreateCommand::Tool { id, agent } => {
                cmd_create(&repo_root, CapabilityType::Tool, &id, &agent)
            }
            CreateCommand::Hook { id, agent } => {
                cmd_create(&repo_root, CapabilityType::Hook, &id, &agent)
            }
            CreateCommand::Workflow { id, agent } => {
                cmd_create(&repo_root, CapabilityType::Workflow, &id, &agent)
            }
        },
        Some(Command::Add {
            source,
            name,
            agent,
            global,
            kind,
        }) => match kind {
            None => cmd_add(
                &repo_root,
                source.as_deref(),
                name.as_deref(),
                None,
                &agent,
                global,
                None,
            ),
            Some(AddCommand::Skill {
                source: typed_source,
                name: typed_name,
                agent: typed_agent,
                global: typed_global,
            }) => {
                reject_parent_add_options(source.as_ref(), name.as_ref(), &agent, global)?;
                cmd_add(
                    &repo_root,
                    Some(typed_source.as_path()),
                    typed_name.as_deref(),
                    Some("skill"),
                    &typed_agent,
                    typed_global,
                    None,
                )
            }
            Some(AddCommand::Tool {
                source: typed_source,
                name: typed_name,
                agent: typed_agent,
                global: typed_global,
            }) => {
                reject_parent_add_options(source.as_ref(), name.as_ref(), &agent, global)?;
                cmd_add(
                    &repo_root,
                    Some(typed_source.as_path()),
                    typed_name.as_deref(),
                    Some("tool"),
                    &typed_agent,
                    typed_global,
                    None,
                )
            }
            Some(AddCommand::Hook {
                source: typed_source,
                name: typed_name,
                hook_file,
                agent: typed_agent,
                global: typed_global,
            }) => {
                reject_parent_add_options(source.as_ref(), name.as_ref(), &agent, global)?;
                cmd_add(
                    &repo_root,
                    Some(typed_source.as_path()),
                    typed_name.as_deref(),
                    Some("hook"),
                    &typed_agent,
                    typed_global,
                    hook_file.as_deref(),
                )
            }
            Some(AddCommand::Workflow {
                source: typed_source,
                name: typed_name,
                agent: typed_agent,
                global: typed_global,
            }) => {
                reject_parent_add_options(source.as_ref(), name.as_ref(), &agent, global)?;
                cmd_add(
                    &repo_root,
                    Some(typed_source.as_path()),
                    typed_name.as_deref(),
                    Some("workflow"),
                    &typed_agent,
                    typed_global,
                    None,
                )
            }
        },
        Some(Command::List { scope, kind }) => cmd_list(&repo_root, &scope, kind.as_deref()),
        Some(Command::Status) => cmd_status(&repo_root),
        Some(Command::Generate { artifact }) => match artifact {
            GenerateCommand::Index { agent, output } => {
                cmd_generate_index(&repo_root, agent.as_deref(), output.as_deref())
            }
            GenerateCommand::Report { output } => {
                cmd_generate_report(&repo_root, output.as_deref())
            }
        },
        Some(Command::Outdated) => cmd_outdated(&repo_root),
        Some(Command::Diff {
            capability_id,
            agent,
            upstream,
        }) => cmd_diff(&repo_root, &capability_id, agent.as_deref(), upstream),
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
        Some(Command::Delete {
            id,
            scope,
            agent,
            force,
        }) => cmd_delete(&repo_root, &id, &scope, &agent, force),
        Some(Command::Untrack { id, scope, agent }) => cmd_untrack(&repo_root, &id, &scope, &agent),
        Some(Command::Agent { action }) => match action {
            AgentCommand::List { global } => cmd_agent_list(&repo_root, global),
            AgentCommand::Add { id } => cmd_agent_add(&repo_root, &id),
            AgentCommand::Remove { id } => cmd_agent_remove(&repo_root, &id),
            AgentCommand::SetDefault { id, global } => {
                cmd_agent_set_default(&repo_root, &id, global)
            }
        },
    }
}
