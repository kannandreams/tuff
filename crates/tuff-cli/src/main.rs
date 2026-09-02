mod adapter;
mod adapters;
mod commands;
mod display;
mod mcp_client;

pub use tuff_core::{
    cache, catalog, check, config, error, git, lockfile, manifest, oci, pack, paths, resolver,
    tool, tree_diff,
};

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use commands::{
    PackBuildOptions, PackInitOptions, cmd_add, cmd_add_mcp, cmd_add_pack, cmd_agent_add,
    cmd_agent_list, cmd_agent_remove, cmd_agent_set_default, cmd_cache_clear, cmd_check,
    cmd_create, cmd_delete, cmd_diff, cmd_generate_index, cmd_generate_report,
    cmd_hooks_check_portability, cmd_hooks_matrix, cmd_init, cmd_list, cmd_lock_migrate,
    cmd_mcp_doctor, cmd_outdated, cmd_pack_build, cmd_pack_check, cmd_pack_extract, cmd_pack_init,
    cmd_pack_inspect, cmd_pack_pull, cmd_pack_push, cmd_pack_verify, cmd_status, cmd_untrack,
    cmd_update,
};
use error::{Result, TuffError};
use manifest::CapabilityType;

#[derive(Parser)]
#[command(name = "tuff", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize project Tuff state.
    Init {
        /// Initialize global scope.
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

        /// Install to global scope.
        #[arg(short = 'g', long = "global")]
        global: bool,

        #[command(subcommand)]
        kind: Option<AddCommand>,
    },

    /// Build, inspect, verify, and extract capability packs.
    Pack {
        #[command(subcommand)]
        action: PackCommand,
    },

    /// List installed capabilities.
    List {
        /// Filter by scope: project, global, or all.
        #[arg(short = 's', long = "scope", default_value = "all")]
        scope: String,

        /// Filter by capability type: skill, tool, hook, workflow, mcp-server.
        #[arg(short = 'p', long = "type")]
        kind: Option<String>,
    },

    /// Show detailed status for installed primitives.
    Status,

    /// Generate derived Tuff artifacts.
    Generate {
        #[command(subcommand)]
        artifact: GenerateCommand,
    },

    /// Show installed capabilities with upstream update status.
    Outdated {
        /// Use unencrypted HTTP for a development registry, when checking a
        /// pack-sourced capability.
        #[arg(long)]
        plain_http: bool,
        /// Additional PEM certificate authority to trust (repeatable), when
        /// checking a pack-sourced capability.
        #[arg(long = "ca-file")]
        ca_file: Vec<PathBuf>,
    },

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

        /// Diff output format.
        #[arg(long = "format", value_enum, default_value_t = commands::DiffFormat::Unified)]
        format: commands::DiffFormat,
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

        /// Force overwrite local changes with upstream (Git and pack sources).
        #[arg(short = 'f', long = "force")]
        force: bool,

        /// For a pack-installed capability: update the whole pack from this
        /// artifact instead of resolving its registry.
        #[arg(long = "pack", value_name = "ARTIFACT")]
        pack: Option<PathBuf>,

        /// Use unencrypted HTTP for a development registry, when updating a
        /// pack-installed capability.
        #[arg(long)]
        plain_http: bool,

        /// Additional PEM certificate authority to trust (repeatable), when
        /// updating a pack-installed capability.
        #[arg(long = "ca-file")]
        ca_file: Vec<PathBuf>,
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

    /// Delete Tuff-generated capability files.
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

    /// Inspect hook compatibility and portability.
    Hooks {
        #[command(subcommand)]
        action: HooksCommand,
    },

    /// Manage Tuff's disposable machine-local cache.
    Cache {
        #[command(subcommand)]
        action: CacheCommand,
    },

    /// Inspect and migrate the project lockfile.
    Lock {
        #[command(subcommand)]
        action: LockCommand,
    },

    /// Diagnose installed MCP server capabilities.
    Mcp {
        #[command(subcommand)]
        action: McpCommand,
    },
}

#[derive(Subcommand)]
enum McpCommand {
    /// Spawn each installed mcp-server capability, complete the MCP
    /// initialize handshake, and list its tools.
    Doctor {
        /// Only check servers wired into this agent (repeatable).
        #[arg(short = 'a', long = "agent")]
        agent: Vec<String>,
        /// Check global scope instead of project scope.
        #[arg(short = 'g', long = "global")]
        global: bool,
        /// Output results as JSON.
        #[arg(long = "json")]
        json: bool,
        /// Report failures but exit with code 0.
        #[arg(long = "ignore-failures")]
        ignore_failures: bool,
        /// Seconds to wait for a server to respond before reporting a timeout.
        #[arg(long = "timeout", default_value_t = 10)]
        timeout_secs: u64,
    },
}

#[derive(Subcommand)]
enum CacheCommand {
    /// Delete all disposable cached materialized trees and source clones.
    Clear,
}

#[derive(Subcommand)]
enum LockCommand {
    /// Rewrite tuff.lock in the current schema version, changing nothing else.
    Migrate,
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
        /// Output path. Defaults to tuff-report.md.
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
    /// MCP servers are not scaffolded; use `tuff add mcp` instead.
    #[command(name = "mcp-server")]
    McpServer {
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
        /// Install to global scope.
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
    /// Install every capability in a verified pack artifact.
    Pack {
        source: PathBuf,
        #[arg(short = 'a', long = "agent")]
        agent: Vec<String>,
        /// OCI reference this pack was pulled from (e.g.
        /// ghcr.io/acme/engineering:1.2.0), recorded so `tuff outdated` can
        /// check whether a newer pack version has been published. Omit if
        /// the pack did not come from a registry, or you do not want it
        /// checked.
        #[arg(long = "reference")]
        reference: Option<String>,
    },
    /// Install external MCP servers from the built-in catalog, a local path,
    /// or a git URL — and wire each into every selected harness's MCP config.
    Mcp {
        /// Catalog ids (see `tuff add mcp --help`), paths to a directory
        /// with a tuff.toml, or git URLs. Several may be given at once.
        #[arg(required = true)]
        sources: Vec<String>,
        /// Agent harness to emit for (repeatable).
        #[arg(short = 'a', long = "agent")]
        agent: Vec<String>,
        /// Install to global scope.
        #[arg(short = 'g', long = "global")]
        global: bool,
        /// Skip the interactive prompt for a different env var name per
        /// catalog entry and accept the catalog's defaults. Implied when
        /// stdin isn't a terminal (scripts, CI).
        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum PackCommand {
    /// Create a source pack manifest.
    Init {
        /// Stable pack name, optionally namespaced with slashes.
        name: String,
        /// Select tracked capabilities from the current project.
        #[arg(long = "from-project")]
        from_project: bool,
        /// Tracked capability to include (repeatable; workflows add their requirements).
        #[arg(short = 'c', long = "capability")]
        capability: Vec<String>,
        /// Agent harness to render (repeatable; defaults to the project default).
        #[arg(short = 'a', long = "agent")]
        agent: Vec<String>,
        /// Initial pack version.
        #[arg(long)]
        version: Option<String>,
        /// Pack description.
        #[arg(long)]
        description: Option<String>,
    },
    /// Validate a source pack without writing an artifact.
    Check {
        /// Pack directory or tuff-pack.toml path.
        path: Option<PathBuf>,
    },
    /// Build a deterministic local pack artifact.
    Build {
        /// Pack directory or tuff-pack.toml path.
        path: Option<PathBuf>,
        /// Build tracked capabilities directly from the current project.
        #[arg(long)]
        name: Option<String>,
        /// Pack version (project mode defaults to 0.1.0).
        #[arg(long)]
        version: Option<String>,
        /// Pack description for a one-shot project build.
        #[arg(long)]
        description: Option<String>,
        /// Tracked capability to include (repeatable; workflows add their requirements).
        #[arg(short = 'c', long = "capability")]
        capability: Vec<String>,
        /// Agent harness to render (repeatable; defaults to the project default).
        #[arg(short = 'a', long = "agent")]
        agent: Vec<String>,
        /// Artifact output path.
        #[arg(short = 'o', long = "output")]
        output: Option<PathBuf>,
    },
    /// Print verified pack metadata.
    Inspect {
        /// Pack artifact path.
        artifact: PathBuf,
        /// Print canonical metadata as JSON.
        #[arg(long = "json")]
        json: bool,
    },
    /// Verify the complete pack artifact and every stored file.
    Verify {
        /// Pack artifact path.
        artifact: PathBuf,
    },
    /// Publish a verified pack artifact to an OCI registry tag.
    Push {
        /// Local pack artifact to publish.
        artifact: PathBuf,
        /// OCI registry reference with an explicit tag.
        reference: String,
        /// Replace an existing tag that points to different content.
        #[arg(long)]
        force: bool,
        /// Use unencrypted HTTP for a development registry.
        #[arg(long)]
        plain_http: bool,
        /// Additional PEM certificate authority to trust (repeatable).
        #[arg(long = "ca-file")]
        ca_file: Vec<PathBuf>,
        /// Print deterministic JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Pull and verify a pack artifact from an OCI registry.
    Pull {
        /// OCI registry reference with an explicit tag or digest.
        reference: String,
        /// New local artifact path. Existing files are never overwritten.
        #[arg(short = 'o', long = "output")]
        output: PathBuf,
        /// Use unencrypted HTTP for a development registry.
        #[arg(long)]
        plain_http: bool,
        /// Additional PEM certificate authority to trust (repeatable).
        #[arg(long = "ca-file")]
        ca_file: Vec<PathBuf>,
        /// Print deterministic JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Extract one pre-rendered target into a missing or empty directory.
    Extract {
        /// Pack artifact path.
        artifact: PathBuf,
        /// Target agent contained in the artifact.
        #[arg(short = 'a', long = "agent")]
        agent: String,
        /// Missing or empty output directory.
        #[arg(short = 'o', long = "output")]
        output: PathBuf,
    },
}

fn reject_parent_add_options(
    source: Option<&PathBuf>,
    name: Option<&String>,
    agent: &[String],
    global: bool,
) -> Result<()> {
    if source.is_some() || name.is_some() || !agent.is_empty() || global {
        return Err(TuffError::new(
            "for typed 'tuff add' commands, put --agent and --global after the capability source",
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

        /// Set the default for global operations.
        #[arg(short = 'g', long = "global")]
        global: bool,
    },
}

#[derive(Subcommand)]
enum HooksCommand {
    /// Print hook compatibility for registered agents.
    Matrix,

    /// Check whether a tracked hook can render on a target agent.
    CheckPortability {
        /// Installed hook capability id.
        id: String,

        /// Registered target adapter id.
        #[arg(long = "target")]
        target: String,
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
            CreateCommand::McpServer { id, agent } => {
                cmd_create(&repo_root, CapabilityType::McpServer, &id, &agent)
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
            Some(AddCommand::Pack {
                source: typed_source,
                agent: typed_agent,
                reference: typed_reference,
            }) => {
                reject_parent_add_options(source.as_ref(), name.as_ref(), &agent, global)?;
                cmd_add_pack(
                    &repo_root,
                    &typed_source,
                    &typed_agent,
                    typed_reference.as_deref(),
                )
            }
            Some(AddCommand::Mcp {
                sources,
                agent: typed_agent,
                global: typed_global,
                yes,
            }) => {
                reject_parent_add_options(source.as_ref(), name.as_ref(), &agent, global)?;
                cmd_add_mcp(&repo_root, &sources, &typed_agent, typed_global, yes)
            }
        },
        Some(Command::Pack { action }) => match action {
            PackCommand::Init {
                name,
                from_project,
                capability,
                agent,
                version,
                description,
            } => cmd_pack_init(
                &repo_root,
                PackInitOptions {
                    name,
                    from_project,
                    capabilities: capability,
                    agents: agent,
                    version,
                    description,
                },
            ),
            PackCommand::Check { path } => cmd_pack_check(&repo_root, path.as_deref()),
            PackCommand::Build {
                path,
                name,
                version,
                description,
                capability,
                agent,
                output,
            } => cmd_pack_build(
                &repo_root,
                PackBuildOptions {
                    path,
                    name,
                    version,
                    description,
                    capabilities: capability,
                    agents: agent,
                    output,
                },
            ),
            PackCommand::Inspect { artifact, json } => cmd_pack_inspect(&artifact, json),
            PackCommand::Verify { artifact } => cmd_pack_verify(&artifact),
            PackCommand::Push {
                artifact,
                reference,
                force,
                plain_http,
                ca_file,
                json,
            } => cmd_pack_push(&artifact, &reference, force, plain_http, &ca_file, json),
            PackCommand::Pull {
                reference,
                output,
                plain_http,
                ca_file,
                json,
            } => cmd_pack_pull(&reference, &output, plain_http, &ca_file, json),
            PackCommand::Extract {
                artifact,
                agent,
                output,
            } => cmd_pack_extract(&artifact, &agent, &output),
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
        Some(Command::Outdated {
            plain_http,
            ca_file,
        }) => cmd_outdated(&repo_root, plain_http, &ca_file),
        Some(Command::Diff {
            capability_id,
            agent,
            upstream,
            format,
        }) => cmd_diff(
            &repo_root,
            &capability_id,
            agent.as_deref(),
            upstream,
            format,
        ),
        Some(Command::Update {
            id,
            scope,
            check,
            agent,
            force,
            pack,
            plain_http,
            ca_file,
        }) => cmd_update(
            &repo_root,
            &id,
            commands::UpdateOptions {
                scope: scope.as_deref(),
                requested_targets: &agent,
                check,
                force,
                pack_artifact: pack.as_deref(),
                oci_options: tuff_core::oci::OciTransferOptions {
                    plain_http,
                    ca_files: ca_file,
                },
            },
        ),
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
        Some(Command::Hooks { action }) => match action {
            HooksCommand::Matrix => cmd_hooks_matrix(&repo_root),
            HooksCommand::CheckPortability { id, target } => {
                cmd_hooks_check_portability(&repo_root, &id, &target)
            }
        },
        Some(Command::Cache {
            action: CacheCommand::Clear,
        }) => cmd_cache_clear(),
        Some(Command::Lock {
            action: LockCommand::Migrate,
        }) => cmd_lock_migrate(&repo_root),
        Some(Command::Mcp { action }) => match action {
            McpCommand::Doctor {
                agent,
                global,
                json,
                ignore_failures,
                timeout_secs,
            } => cmd_mcp_doctor(
                &repo_root,
                &agent,
                global,
                json,
                ignore_failures,
                timeout_secs,
            ),
        },
    }
}
