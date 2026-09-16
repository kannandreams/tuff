mod adapter;
mod adapters;
mod commands;
mod display;
mod mcp_client;
mod mcp_http;

pub use tuff_core::{
    cache, catalog, check, config, error, git, lockfile, manifest, oci, pack, paths, registry,
    release, resolver, tool, tree_diff,
};

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use commands::{
    PackBuildOptions, PackInitOptions, cmd_add, cmd_add_accepting, cmd_add_mcp, cmd_add_pack,
    cmd_agent_add, cmd_agent_list, cmd_agent_remove, cmd_agent_set_default, cmd_cache_clear,
    cmd_check, cmd_create, cmd_dashboard_publish, cmd_delete, cmd_diff, cmd_generate_index,
    cmd_generate_report, cmd_hooks_check_portability, cmd_hooks_matrix, cmd_hooks_spec, cmd_init,
    cmd_list, cmd_lock_migrate, cmd_mcp_catalog, cmd_mcp_doctor, cmd_mcp_search, cmd_outdated,
    cmd_pack_build, cmd_pack_check, cmd_pack_extract, cmd_pack_init, cmd_pack_inspect,
    cmd_pack_pull, cmd_pack_push, cmd_pack_verify, cmd_policy_matrix, cmd_scan, cmd_status,
    cmd_untrack, cmd_update,
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

        /// Harness to emit for (repeatable).
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
        agent: Vec<String>,

        /// Install to global scope.
        #[arg(short = 'g', long = "global")]
        global: bool,

        /// For a policy: install the rules each harness enforces and record the
        /// rest in tuff.lock, instead of refusing the policy.
        #[arg(long = "accept-unenforced")]
        accept_unenforced: bool,

        #[command(subcommand)]
        kind: Option<AddCommand>,
    },

    /// Find capabilities already in a harness folder that Tuff is not tracking.
    Scan {
        /// Track what the scan found. Without paths, every untracked capability.
        #[arg(long = "adopt")]
        adopt: bool,

        /// Capability directory to adopt (repeatable; requires --adopt).
        paths: Vec<PathBuf>,

        /// Output rows as JSON.
        #[arg(long = "json")]
        json: bool,
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

        /// Filter by capability type: skill, tool, hook, mcp-server, policy.
        #[arg(short = 'p', long = "type")]
        kind: Option<String>,

        /// Output rows as JSON.
        #[arg(long = "json")]
        json: bool,
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

        /// Output rows as JSON.
        #[arg(long = "json")]
        json: bool,
    },

    /// Diff an installed capability against baseline.
    Diff {
        /// Installed capability id.
        capability_id: String,

        /// Harness to diff (defaults to the configured harness).
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
        agent: Option<String>,

        /// Diff against latest upstream source instead of baseline.
        #[arg(short = 'u', long = "upstream")]
        upstream: bool,

        /// Diff output format.
        #[arg(long = "format", value_enum, default_value_t = commands::DiffFormat::Unified)]
        format: commands::DiffFormat,

        /// Output the diff as JSON; the same as `--format json`.
        #[arg(long = "json", conflicts_with = "format")]
        json: bool,
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

        /// Harness to update (repeatable; defaults to the configured harness).
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
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

        /// Also fail while a policy rule is recorded as not enforced for a harness.
        #[arg(long = "strict")]
        strict: bool,
    },

    /// Delete Tuff-generated capability files.
    Delete {
        /// Capability id to delete.
        id: String,

        /// Scope to delete from.
        #[arg(short = 's', long = "scope", default_value = "project")]
        scope: String,

        /// Harness to delete from (repeatable).
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
        agent: Vec<String>,

        /// Delete files even when they have local modifications.
        #[arg(short = 'f', long = "force")]
        force: bool,
    },

    /// Stop tracking a capability without deleting its harness files.
    Untrack {
        /// Capability id to untrack.
        id: String,

        /// Scope to untrack from.
        #[arg(short = 's', long = "scope", default_value = "project")]
        scope: String,

        /// Harness to untrack (repeatable).
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
        agent: Vec<String>,
    },

    /// Manage the harnesses this project installs for.
    Harness {
        #[command(subcommand)]
        action: HarnessCommand,
    },

    /// Inspect hook compatibility and portability.
    Hooks {
        #[command(subcommand)]
        action: HooksCommand,
    },

    /// Inspect what each harness can enforce of a policy.
    Policy {
        #[command(subcommand)]
        action: PolicyCommand,
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

    /// Browse the MCP catalog and registry, and diagnose installed servers.
    Mcp {
        #[command(subcommand)]
        action: McpCommand,
    },

    /// Report this project's capabilities to a dashboard server.
    Dashboard {
        #[command(subcommand)]
        action: DashboardCommand,
    },
}

#[derive(Subcommand)]
enum DashboardCommand {
    /// Build this project's report for a dashboard server.
    Publish {
        /// Report every project with a tuff.lock under this folder.
        #[arg(long = "all")]
        all: bool,

        /// Also report newer versions (needs the network).
        #[arg(long = "outdated")]
        outdated: bool,

        /// Name the project's repository, required outside git or without an origin remote.
        #[arg(long = "project", value_name = "NAME")]
        project: Option<String>,

        /// Print the report and send nothing.
        #[arg(long = "dry-run")]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum McpCommand {
    /// List the built-in MCP server catalog.
    Catalog {
        /// Output the catalog as JSON.
        #[arg(long = "json")]
        json: bool,
    },

    /// Spawn each installed mcp-server capability, complete the MCP
    /// initialize handshake, and list its tools.
    Doctor {
        /// Only check servers wired into this harness (repeatable).
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
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

    /// Search the MCP registry for servers to install.
    Search {
        /// What to search for: part of a name, or a word from a description.
        query: String,
        /// Maximum results to show.
        #[arg(long = "limit", default_value_t = 20)]
        limit: usize,
        /// Registry to search instead of the official one.
        #[arg(long = "registry", default_value = tuff_core::registry::DEFAULT_REGISTRY)]
        registry: String,
        /// Output results as JSON.
        #[arg(long = "json")]
        json: bool,
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
        /// Harness to generate an index for (defaults to the configured harness).
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
        agent: Option<String>,

        /// Output path. Defaults to the harness's standard CAPABILITIES.md path.
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
        /// Harnesses to scaffold for (repeatable).
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
        agent: Vec<String>,
    },
    /// Create and track a tool.
    Tool {
        /// Capability id.
        id: String,
        /// Harnesses to scaffold for (repeatable).
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
        agent: Vec<String>,
    },
    /// Create and track a hook.
    Hook {
        /// Capability id.
        id: String,
        /// Harnesses to scaffold for (repeatable).
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
        agent: Vec<String>,
    },
    /// MCP servers are not scaffolded; use `tuff add mcp` instead.
    #[command(name = "mcp-server")]
    McpServer {
        /// Capability id.
        id: String,
        /// Harnesses to scaffold for (repeatable).
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
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
        /// Harness to emit for (repeatable).
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
        agent: Vec<String>,
        /// Install to global scope.
        #[arg(short = 'g', long = "global")]
        global: bool,
    },
    /// Install a tool from a local path or git URL.
    Tool {
        source: PathBuf,
        name: Option<String>,
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
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
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
        agent: Vec<String>,
        #[arg(short = 'g', long = "global")]
        global: bool,
    },
    /// Install every capability in a verified pack artifact.
    Pack {
        source: PathBuf,
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
        agent: Vec<String>,
        /// OCI reference this pack was pulled from (e.g.
        /// ghcr.io/acme/engineering:1.2.0), recorded so `tuff outdated` can
        /// check whether a newer pack version has been published. Omit if
        /// the pack did not come from a registry, or you do not want it
        /// checked.
        #[arg(long = "reference")]
        reference: Option<String>,
    },
    /// Install external MCP servers from the built-in catalog, the MCP
    /// registry, a local path, or a git URL, wiring each into every selected
    /// harness's MCP config.
    Mcp {
        /// Built-in catalog ids, MCP registry names (see `tuff mcp
        /// search`), paths to a directory with a tuff.toml, or git URLs.
        /// Several may be given at once.
        #[arg(required = true)]
        sources: Vec<String>,
        /// Harness to emit for (repeatable).
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
        agent: Vec<String>,
        /// Install to global scope.
        #[arg(short = 'g', long = "global")]
        global: bool,
        /// Skip the interactive prompt for a different env var name per
        /// catalog entry and accept the catalog's defaults. Implied when
        /// stdin isn't a terminal (scripts, CI).
        #[arg(short = 'y', long = "yes")]
        yes: bool,
        /// Registry to resolve a name that is not a built-in catalog id.
        #[arg(long = "registry", default_value = tuff_core::registry::DEFAULT_REGISTRY)]
        registry: String,
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
        /// Tracked capability to include (repeatable).
        #[arg(short = 'c', long = "capability")]
        capability: Vec<String>,
        /// Harness to render (repeatable; defaults to the project default).
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
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
        /// Tracked capability to include (repeatable).
        #[arg(short = 'c', long = "capability")]
        capability: Vec<String>,
        /// Harness to render (repeatable; defaults to the project default).
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
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
        /// Harness contained in the artifact.
        #[arg(short = 'a', long = "harness", value_name = "HARNESS")]
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
    accept_unenforced: bool,
) -> Result<()> {
    if accept_unenforced {
        return Err(TuffError::usage(
            "--accept-unenforced applies to 'tuff add <path>' of a policy, not to typed 'tuff add' commands",
        ));
    }
    if source.is_some() || name.is_some() || !agent.is_empty() || global {
        return Err(TuffError::usage(
            "for typed 'tuff add' commands, put --harness and --global after the capability source",
        ));
    }
    Ok(())
}

#[derive(Subcommand)]
enum HarnessCommand {
    /// List available and registered harnesses.
    List {
        /// Show the global harness configuration.
        #[arg(short = 'g', long = "global")]
        global: bool,
    },

    /// Register a harness for this repo.
    Add {
        /// Harness adapter id.
        id: String,
    },

    /// Unregister a harness without changing installed capabilities.
    Remove {
        /// Harness adapter id.
        id: String,
    },

    /// Set the default harness used when --harness is omitted.
    SetDefault {
        /// Harness adapter id.
        id: String,

        /// Set the default for global operations.
        #[arg(short = 'g', long = "global")]
        global: bool,
    },
}

#[derive(Subcommand)]
enum PolicyCommand {
    /// Print, for every harness, how each kind of policy rule is enforced.
    Matrix {
        /// Output the matrix as JSON.
        #[arg(long = "json")]
        json: bool,
    },
}

#[derive(Subcommand)]
enum HooksCommand {
    /// Print hook compatibility for registered harnesses.
    Matrix,

    /// Print the hook specification this binary implements, for every harness.
    Spec {
        /// Output the specification document as JSON.
        #[arg(long = "json")]
        json: bool,
    },

    /// Check whether a tracked hook can render on a target harness.
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
        report_error(&error, wants_json());
        std::process::exit(error.exit_code());
    }
}

/// Whether the invocation asked for machine-readable output.
///
/// Read from the raw arguments rather than the parsed command: an error can
/// happen before or during parsing, and a `--json` caller wants one shape
/// on stdout and one shape on stderr, not prose on failure.
fn wants_json() -> bool {
    std::env::args().any(|arg| arg == "--json")
}

fn report_error(error: &TuffError, json: bool) {
    if json {
        let mut envelope = serde_json::json!({
            "error": {
                "kind": error.kind().as_str(),
                "message": error.message(),
            }
        });
        if let Some(hint) = error.hint() {
            envelope["error"]["hint"] = serde_json::Value::String(hint.to_string());
        }
        eprintln!("{envelope}");
        return;
    }
    eprintln!("error: {error}");
    if let Some(hint) = error.hint() {
        eprintln!("hint: {hint}");
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
            CreateCommand::McpServer { id, agent } => {
                cmd_create(&repo_root, CapabilityType::McpServer, &id, &agent)
            }
        },
        Some(Command::Add {
            source,
            name,
            agent,
            global,
            accept_unenforced,
            kind,
        }) => match kind {
            None => cmd_add_accepting(
                &repo_root,
                source.as_deref(),
                name.as_deref(),
                None,
                &agent,
                global,
                None,
                accept_unenforced,
            ),
            Some(AddCommand::Skill {
                source: typed_source,
                name: typed_name,
                agent: typed_agent,
                global: typed_global,
            }) => {
                reject_parent_add_options(
                    source.as_ref(),
                    name.as_ref(),
                    &agent,
                    global,
                    accept_unenforced,
                )?;
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
                reject_parent_add_options(
                    source.as_ref(),
                    name.as_ref(),
                    &agent,
                    global,
                    accept_unenforced,
                )?;
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
                reject_parent_add_options(
                    source.as_ref(),
                    name.as_ref(),
                    &agent,
                    global,
                    accept_unenforced,
                )?;
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
            Some(AddCommand::Pack {
                source: typed_source,
                agent: typed_agent,
                reference: typed_reference,
            }) => {
                reject_parent_add_options(
                    source.as_ref(),
                    name.as_ref(),
                    &agent,
                    global,
                    accept_unenforced,
                )?;
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
                registry,
            }) => {
                reject_parent_add_options(
                    source.as_ref(),
                    name.as_ref(),
                    &agent,
                    global,
                    accept_unenforced,
                )?;
                cmd_add_mcp(
                    &repo_root,
                    &sources,
                    &typed_agent,
                    typed_global,
                    yes,
                    &registry,
                )
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
        Some(Command::List { scope, kind, json }) => {
            cmd_list(&repo_root, &scope, kind.as_deref(), json)
        }
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
            json,
        }) => cmd_outdated(&repo_root, plain_http, &ca_file, json),
        Some(Command::Diff {
            capability_id,
            agent,
            upstream,
            format,
            json,
        }) => cmd_diff(
            &repo_root,
            &capability_id,
            agent.as_deref(),
            upstream,
            if json {
                commands::DiffFormat::Json
            } else {
                format
            },
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
            strict,
        }) => cmd_check(&repo_root, json, ignore_failures, global, strict),
        Some(Command::Delete {
            id,
            scope,
            agent,
            force,
        }) => cmd_delete(&repo_root, &id, &scope, &agent, force),
        Some(Command::Scan { adopt, paths, json }) => cmd_scan(&repo_root, adopt, &paths, json),
        Some(Command::Untrack { id, scope, agent }) => cmd_untrack(&repo_root, &id, &scope, &agent),
        Some(Command::Harness { action }) => match action {
            HarnessCommand::List { global } => cmd_agent_list(&repo_root, global),
            HarnessCommand::Add { id } => cmd_agent_add(&repo_root, &id),
            HarnessCommand::Remove { id } => cmd_agent_remove(&repo_root, &id),
            HarnessCommand::SetDefault { id, global } => {
                cmd_agent_set_default(&repo_root, &id, global)
            }
        },
        Some(Command::Hooks { action }) => match action {
            HooksCommand::Matrix => cmd_hooks_matrix(&repo_root),
            HooksCommand::Spec { json } => cmd_hooks_spec(json),
            HooksCommand::CheckPortability { id, target } => {
                cmd_hooks_check_portability(&repo_root, &id, &target)
            }
        },
        Some(Command::Policy { action }) => match action {
            PolicyCommand::Matrix { json } => cmd_policy_matrix(json),
        },
        Some(Command::Cache {
            action: CacheCommand::Clear,
        }) => cmd_cache_clear(),
        Some(Command::Lock {
            action: LockCommand::Migrate,
        }) => cmd_lock_migrate(&repo_root),
        Some(Command::Dashboard { action }) => match action {
            DashboardCommand::Publish {
                all,
                outdated,
                project,
                dry_run,
            } => cmd_dashboard_publish(
                &repo_root,
                commands::PublishOptions {
                    all,
                    outdated,
                    project: project.as_deref(),
                    dry_run,
                },
            ),
        },
        Some(Command::Mcp { action }) => match action {
            McpCommand::Catalog { json } => cmd_mcp_catalog(json),
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
            McpCommand::Search {
                query,
                limit,
                registry,
                json,
            } => cmd_mcp_search(&query, limit, &registry, json),
        },
    }
}
