use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::adapter::{AdapterKind, AgentAdapter, resolve_capability};
use crate::error::{Result, TuffError};
use crate::lockfile::{self, PackLockEntry, PackProvenance, PackSource};
use crate::manifest;
use crate::oci::{self, OciPushStatus, OciTransferOptions};
use crate::pack::{
    self, LoadedPack, PackArtifact, PackArtifactCapability, PackArtifactContent,
    PackArtifactMetadata,
};
use crate::resolver::Scope;

use tuff_packs::build::{Built, canonical_target_ids, validate_pack_targets};
use tuff_packs::import::unsupported_of;
use tuff_packs::preview::{artifact_target, dirty_pack_members};

use super::add::install_capability;
use super::block_on_oci;
use super::capability_index;
use super::project_pack::{
    PreparedProjectPack, default_project_capabilities, prepare_project_pack,
};
use super::resolve_agent_selection;

pub struct PackInitOptions {
    pub name: String,
    pub from_project: bool,
    pub capabilities: Vec<String>,
    pub agents: Vec<String>,
    pub version: Option<String>,
    pub description: Option<String>,
}

pub struct PackBuildOptions {
    pub path: Option<PathBuf>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub capabilities: Vec<String>,
    pub agents: Vec<String>,
    pub output: Option<PathBuf>,
    /// Build bundle targets without the members they cannot hold.
    pub skip_unsupported: bool,
}

pub fn cmd_pack_init(root: &Path, options: PackInitOptions) -> Result<()> {
    validate_pack_name(&options.name)?;
    if options.from_project {
        return init_project_pack(root, options);
    }
    if !options.capabilities.is_empty()
        || !options.agents.is_empty()
        || options.version.is_some()
        || options.description.is_some()
    {
        return Err(TuffError::usage(
            "--capability, --harness, --version, and --description require --from-project",
        ));
    }
    let manifest_path = root.join(pack::PACK_MANIFEST_FILE);
    if manifest_path.exists() {
        return Err(TuffError::refused(format!(
            "refusing to overwrite existing pack manifest: {}",
            manifest_path.display()
        )));
    }
    let manifest = pack::PackManifest {
        schema: pack::PACK_SCHEMA_VERSION,
        name: options.name,
        version: "0.1.0".into(),
        description: "Describe the capabilities and runtime contract in this pack.".into(),
        homepage: None,
        license: None,
        keywords: Vec::new(),
        author: None,
        import: None,
        x: Default::default(),
        build: pack::PackBuild {
            targets: vec!["open-agents".into()],
        },
        project: None,
        capabilities: vec![pack::PackMember {
            path: "capabilities/replace-me".into(),
        }],
    };
    fs::create_dir_all(root.join("capabilities"))?;
    pack::write_manifest(&manifest_path, &manifest)?;
    println!("created {}", manifest_path.display());
    println!(
        "next: add capability paths, then run `tuff pack check {}`",
        root.display()
    );
    Ok(())
}

pub fn cmd_pack_check(repo_root: &Path, path: Option<&Path>) -> Result<()> {
    let path = path.unwrap_or_else(|| Path::new("."));
    let (_, manifest) = pack::load_manifest(path)?;
    if manifest.project.is_some() {
        let prepared = prepare_project_pack(repo_root, manifest)?;
        validate_pack_targets(&prepared.loaded)?;
        validate_project_source_baselines(&prepared)?;
        print_valid_pack(&prepared.loaded);
        return Ok(());
    }
    let loaded = pack::load_pack(path)?;
    validate_pack_targets(&loaded)?;
    print_valid_pack(&loaded);
    Ok(())
}

fn print_valid_pack(pack: &LoadedPack) {
    println!(
        "pack {} {} is valid ({} capabilities, {} targets)",
        pack.manifest.name,
        pack.manifest.version,
        pack.members.len(),
        pack.manifest.build.targets.len()
    );
}

pub fn cmd_pack_build(repo_root: &Path, options: PackBuildOptions) -> Result<()> {
    if let Some(name) = options.name.as_deref() {
        if options.path.is_some() {
            return Err(TuffError::usage(
                "a source-pack path cannot be combined with --name",
            ));
        }
        validate_pack_name(name)?;
        let prepared = prepare_one_shot_project_pack(repo_root, &options)?;
        validate_project_source_baselines(&prepared)?;
        let output = options
            .output
            .unwrap_or_else(|| default_project_artifact_path(repo_root, &prepared.loaded.manifest));
        return build_loaded_pack(&prepared.loaded, &output, options.skip_unsupported);
    }
    if options.version.is_some()
        || options.description.is_some()
        || !options.capabilities.is_empty()
        || !options.agents.is_empty()
    {
        return Err(TuffError::usage(
            "--version, --description, --capability, and --harness require --name for a one-shot project build",
        ));
    }
    let path = options.path.as_deref().unwrap_or_else(|| Path::new("."));
    let (_, manifest) = pack::load_manifest(path)?;
    if manifest.project.is_some() {
        let prepared = prepare_project_pack(repo_root, manifest)?;
        validate_project_source_baselines(&prepared)?;
        let output = options
            .output
            .unwrap_or_else(|| default_project_artifact_path(repo_root, &prepared.loaded.manifest));
        return build_loaded_pack(&prepared.loaded, &output, options.skip_unsupported);
    }
    let loaded = pack::load_pack(path)?;
    let output = options
        .output
        .unwrap_or_else(|| default_artifact_path(&loaded));
    build_loaded_pack(&loaded, &output, options.skip_unsupported)
}

/// Print what a build had to say to standard error.
pub(crate) fn print_build_notes(built: &Built) {
    print_notes(&built.notes, &built.warnings);
}

fn print_notes(notes: &[String], warnings: &[String]) {
    for note in notes {
        eprintln!("note: {note}");
    }
    for warning in warnings {
        eprintln!("warning: {warning}");
    }
}

/// Render a pack with [`tuff_packs::build::render`], printing its notes.
fn render_artifact_with(
    loaded: &LoadedPack,
    skip_unsupported: bool,
) -> Result<(PackArtifactMetadata, Vec<PackArtifactContent>)> {
    let rendered = tuff_packs::build::render(loaded, skip_unsupported)?;
    print_notes(&rendered.notes, &rendered.warnings);
    Ok((rendered.metadata, rendered.contents))
}

fn render_artifact(
    loaded: &LoadedPack,
) -> Result<(PackArtifactMetadata, Vec<PackArtifactContent>)> {
    render_artifact_with(loaded, false)
}

fn printed(built: Built) -> (tempfile::TempDir, PackArtifact) {
    print_build_notes(&built);
    (built.directory, built.artifact)
}

fn build_pack_for_declared_targets(pack_root: &Path) -> Result<(tempfile::TempDir, PackArtifact)> {
    Ok(printed(tuff_packs::build::build_for_declared_targets(
        pack_root,
    )?))
}

fn build_pack_for_targets(
    pack_root: &Path,
    targets: &[String],
) -> Result<(tempfile::TempDir, PackArtifact)> {
    Ok(printed(tuff_packs::build::build_for_targets(
        pack_root, targets,
    )?))
}

/// The source pack a git checkout holds, printing the import's notes.
fn source_pack_of(
    checkout: &tuff_packs::git::GitPackCheckout,
) -> Result<(Option<tempfile::TempDir>, PathBuf)> {
    let source = tuff_packs::import::source_pack_of(checkout)?;
    for note in &source.notes {
        eprintln!("note: {note}");
    }
    Ok((source.directory, source.root))
}

fn build_loaded_pack(loaded: &LoadedPack, output: &Path, skip_unsupported: bool) -> Result<()> {
    if output.exists() {
        return Err(TuffError::refused(format!(
            "refusing to overwrite existing pack artifact: {}",
            output.display()
        )));
    }
    let (metadata, contents) = render_artifact_with(loaded, skip_unsupported)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let digest = pack::write_artifact(output, metadata, contents)?;
    println!(
        "built {} {} -> {}",
        loaded.manifest.name,
        loaded.manifest.version,
        output.display()
    );
    println!("sha256:{digest}");
    println!("next: tuff pack verify {}", output.display());
    Ok(())
}

fn init_project_pack(root: &Path, options: PackInitOptions) -> Result<()> {
    let manifest_path = project_manifest_path(root, &options.name)?;
    if manifest_path.exists() {
        return Err(TuffError::refused(format!(
            "refusing to overwrite existing pack manifest: {}",
            manifest_path.display()
        )));
    }
    let lock = lockfile::require_lockfile(root)?;
    let selected = if options.capabilities.is_empty() {
        default_project_capabilities(&lock)
    } else {
        options.capabilities
    };
    if selected.is_empty() {
        return Err(
            TuffError::usage("this project has no packageable capabilities")
                .with_hint("add a capability first"),
        );
    }
    let targets = resolve_project_agent_selection(root, &options.agents)?;
    let version = options.version.unwrap_or_else(|| "0.1.0".to_string());
    let description = options
        .description
        .unwrap_or_else(|| format!("Project capability pack {}.", options.name));
    let manifest = project_pack_manifest(&options.name, &version, &description, targets, selected);
    let prepared = prepare_project_pack(root, manifest)?;
    validate_project_source_baselines(&prepared)?;
    let expanded = prepared
        .loaded
        .members
        .iter()
        .map(|member| member.manifest.id.clone())
        .collect();
    let saved = project_pack_manifest(
        &prepared.loaded.manifest.name,
        &prepared.loaded.manifest.version,
        &prepared.loaded.manifest.description,
        prepared.loaded.manifest.build.targets.clone(),
        expanded,
    );
    let parent = manifest_path
        .parent()
        .ok_or_else(|| TuffError::usage("pack manifest path has no parent"))?;
    fs::create_dir_all(parent)?;
    pack::write_manifest(&manifest_path, &saved)?;
    println!("created {}", manifest_path.display());
    println!("next: tuff pack check {}", parent.display());
    println!("then: tuff pack build {}", parent.display());
    Ok(())
}

fn prepare_one_shot_project_pack(
    repo_root: &Path,
    options: &PackBuildOptions,
) -> Result<PreparedProjectPack> {
    let name = options
        .name
        .as_deref()
        .ok_or_else(|| TuffError::usage("project pack name is required"))?;
    let lock = lockfile::require_lockfile(repo_root)?;
    let selected = if options.capabilities.is_empty() {
        default_project_capabilities(&lock)
    } else {
        options.capabilities.clone()
    };
    if selected.is_empty() {
        return Err(
            TuffError::usage("this project has no packageable capabilities")
                .with_hint("add a capability first"),
        );
    }
    let targets = resolve_project_agent_selection(repo_root, &options.agents)?;
    let version = options.version.as_deref().unwrap_or("0.1.0");
    let description = options
        .description
        .clone()
        .unwrap_or_else(|| format!("Project capability pack {name}."));
    prepare_project_pack(
        repo_root,
        project_pack_manifest(name, version, &description, targets, selected),
    )
}

fn project_pack_manifest(
    name: &str,
    version: &str,
    description: &str,
    targets: Vec<String>,
    capabilities: Vec<String>,
) -> pack::PackManifest {
    pack::PackManifest {
        schema: pack::PACK_SCHEMA_VERSION,
        name: name.to_string(),
        version: version.to_string(),
        description: description.to_string(),
        homepage: None,
        license: None,
        keywords: Vec::new(),
        author: None,
        import: None,
        x: Default::default(),
        build: pack::PackBuild { targets },
        project: Some(pack::ProjectPackSelection { capabilities }),
        capabilities: Vec::new(),
    }
}

fn validate_project_source_baselines(prepared: &PreparedProjectPack) -> Result<()> {
    let mut targets = BTreeSet::new();
    for member in &prepared.loaded.members {
        let entry = prepared
            .lock
            .capabilities
            .get(&member.manifest.id)
            .ok_or_else(|| {
                TuffError::new("prepared project pack member is missing from tuff.lock")
            })?;
        targets.extend(entry.targets.keys().cloned());
    }
    let mut verification_pack = prepared.loaded.clone();
    verification_pack.manifest.build.targets = targets.into_iter().collect();
    let (metadata, _) = render_artifact(&verification_pack)?;
    for target in &metadata.targets {
        for capability in &target.capabilities {
            let expected = prepared
                .lock
                .capabilities
                .get(&capability.id)
                .and_then(|entry| entry.targets.get(&target.id));
            let Some(expected) = expected else {
                continue;
            };
            if capability.sha256 != expected.sha256 {
                return Err(TuffError::drift(format!(
                    "source for '{}' no longer reproduces its accepted '{}' baseline",
                    capability.id, target.id
                ))
                .with_hint(format!(
                    "run 'tuff update {}' before building the pack",
                    capability.id
                )));
            }
        }
    }
    Ok(())
}

fn validate_pack_name(name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty()
        || name != name.trim_matches('/')
        || name.contains('\\')
        || name.chars().any(char::is_whitespace)
        || name
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(TuffError::usage(
            "pack name must contain safe, non-empty slash-separated components without whitespace",
        ));
    }
    Ok(())
}

fn project_manifest_path(root: &Path, name: &str) -> Result<PathBuf> {
    validate_pack_name(name)?;
    Ok(root
        .join("tuff-packs")
        .join(name)
        .join(pack::PACK_MANIFEST_FILE))
}

fn resolve_project_agent_selection(root: &Path, requested: &[String]) -> Result<Vec<String>> {
    if !requested.is_empty() {
        let (bundles, harnesses): (Vec<String>, Vec<String>) = requested
            .iter()
            .cloned()
            .partition(|target| crate::adapter::bundle_writer(target).is_some());
        let mut targets = if harnesses.is_empty() {
            Vec::new()
        } else {
            resolve_agent_selection(root, &harnesses, false)?
        };
        targets.extend(bundles);
        return Ok(targets);
    }
    let config_path = crate::paths::project_config(root);
    let default_agent = if config_path.is_file() {
        serde_json::from_str::<crate::config::TuffConfig>(&fs::read_to_string(config_path)?)?
            .default_agent
    } else {
        crate::config::DEFAULT_AGENT.to_string()
    };
    resolve_agent_selection(root, &[default_agent], false)
}

fn default_project_artifact_path(root: &Path, manifest: &pack::PackManifest) -> PathBuf {
    let leaf = manifest.name.rsplit('/').next().unwrap_or(&manifest.name);
    root.join("tuff-dist")
        .join(format!("{leaf}-{}.tuffpack", manifest.version))
}

/// Where `tuff pack inspect` reads a pack from.
pub struct PackInspectOptions<'a> {
    /// An artifact path, a source folder or `tuff-pack.toml`, or a git URL.
    pub source: &'a str,
    /// For a git URL: the tag, branch, or commit.
    pub git_ref: Option<&'a str>,
    /// For a git URL: the folder holding `tuff-pack.toml`.
    pub path: Option<&'a str>,
    /// For a marketplace in a git repository: the plugin to read.
    pub plugin: Option<&'a str>,
    pub json: bool,
}

/// Where an inspected pack came from, as the JSON document reports it.
#[derive(serde::Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum InspectedSource {
    Artifact { path: String },
    Folder { path: String },
    Git(lockfile::PackGitSource),
}

pub fn cmd_pack_inspect(options: PackInspectOptions<'_>) -> Result<()> {
    let PackInspectOptions {
        source,
        git_ref,
        path,
        plugin,
        json,
    } = options;
    let as_path = Path::new(source);
    let is_git = crate::git::is_git_url(source);
    if !is_git && (git_ref.is_some() || path.is_some()) {
        return Err(TuffError::usage(
            "--ref and --path apply to a pack inspected from a git address",
        ));
    }
    // A source pack is built in a temporary directory so the document can
    // say what each member writes; nothing in the project changes.
    let (_built, artifact, inspected) = if is_git {
        let checkout = super::pack_git::checkout(super::pack_git::CheckoutRequest {
            url: source,
            path,
            git_ref,
            plugin,
            allow_remote: true,
        })?;
        let (_imported, root) = source_pack_of(&checkout)?;
        let (directory, artifact) = build_pack_for_declared_targets(&root)?;
        (
            Some(directory),
            artifact,
            InspectedSource::Git(checkout.source),
        )
    } else if plugin.is_some() {
        return Err(TuffError::usage(
            "--plugin applies to a git address; for a local plugin, run 'tuff pack import' first",
        ));
    } else if as_path.is_dir()
        || as_path
            .file_name()
            .is_some_and(|name| name == pack::PACK_MANIFEST_FILE)
    {
        let (directory, artifact) = build_pack_for_declared_targets(as_path)?;
        (
            Some(directory),
            artifact,
            InspectedSource::Folder {
                path: source.to_string(),
            },
        )
    } else {
        (
            None,
            pack::read_artifact(as_path)?,
            InspectedSource::Artifact {
                path: source.to_string(),
            },
        )
    };
    if json {
        let document = super::pack_inspect::document(&artifact, &inspected)?;
        println!("{}", serde_json::to_string_pretty(&document)?);
        return Ok(());
    }
    println!("pack: {}", artifact.metadata.name);
    println!("version: {}", artifact.metadata.version);
    println!("digest: sha256:{}", artifact.digest);
    if let InspectedSource::Git(git) = &inspected {
        println!("source: {} ({})", git.url, describe_git_ref(git));
    }
    let members = super::pack_inspect::members(&artifact)?;
    println!("capabilities: {}", members.len());
    for member in &members {
        let runs = if member.runs_code { ", runs code" } else { "" };
        println!(
            "  - {} {} ({}{runs})",
            member.id, member.version, member.capability_type
        );
    }
    println!("targets: {}", artifact.metadata.targets.len());
    for target in &artifact.metadata.targets {
        println!("  - {}", target.id);
    }
    super::pack_import::print_unsupported(unsupported_of(artifact.metadata.import.as_ref()));
    Ok(())
}

pub fn cmd_pack_verify(path: &Path) -> Result<()> {
    let artifact = pack::read_artifact(path)?;
    println!(
        "verified {} {} (sha256:{})",
        artifact.metadata.name, artifact.metadata.version, artifact.digest
    );
    Ok(())
}

pub fn cmd_pack_push(
    artifact: &Path,
    reference: &str,
    force: bool,
    plain_http: bool,
    ca_files: &[PathBuf],
    json: bool,
) -> Result<()> {
    let options = OciTransferOptions {
        plain_http,
        ca_files: ca_files.to_vec(),
    };
    let result = block_on_oci(oci::push_pack(artifact, reference, force, &options))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    let action = match result.status {
        OciPushStatus::Pushed => "pushed",
        OciPushStatus::Unchanged => "unchanged",
    };
    println!(
        "{action} pack {} {} -> {}",
        result.name, result.version, result.tag_reference
    );
    println!("artifact: {}", result.artifact_digest);
    println!("manifest: {}", result.manifest_digest);
    println!("reference: {}", result.reference);
    Ok(())
}

pub fn cmd_pack_pull(
    reference: &str,
    output: &Path,
    plain_http: bool,
    ca_files: &[PathBuf],
    json: bool,
) -> Result<()> {
    let options = OciTransferOptions {
        plain_http,
        ca_files: ca_files.to_vec(),
    };
    let result = block_on_oci(oci::pull_pack(reference, output, &options))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    println!(
        "pulled pack {} {} -> {}",
        result.name, result.version, result.output
    );
    println!("artifact: {}", result.artifact_digest);
    println!("manifest: {}", result.manifest_digest);
    println!("reference: {}", result.reference);
    println!("next: tuff pack verify {}", result.output);
    Ok(())
}

pub fn cmd_pack_extract(path: &Path, agent: &str, output: &Path) -> Result<()> {
    let artifact = pack::read_artifact(path)?;
    if !artifact
        .metadata
        .targets
        .iter()
        .any(|target| target.id == agent)
    {
        return Err(TuffError::usage(format!(
            "pack '{}' has no target '{}'; available targets: {}",
            artifact.metadata.name,
            agent,
            artifact
                .metadata
                .targets
                .iter()
                .map(|target| target.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    let count = pack::extract_prefix(&artifact, &format!("targets/{agent}"), output)?;
    println!(
        "extracted {} {} ({agent}, {count} files) -> {}",
        artifact.metadata.name,
        artifact.metadata.version,
        output.display()
    );
    Ok(())
}

/// What `tuff add pack` was asked to install.
pub struct AddPackOptions<'a> {
    /// A `.tuffpack` path or a git address.
    pub source: &'a str,
    pub targets: &'a [String],
    /// OCI repository an artifact came from (`--reference`).
    pub reference: Option<&'a str>,
    /// Tag, branch, or commit of a git pack (`--ref`).
    pub git_ref: Option<&'a str>,
    /// Repository folder holding a git pack's `tuff-pack.toml` (`--path`).
    pub path: Option<&'a str>,
    /// The marketplace entry to install (`--plugin`).
    pub plugin: Option<&'a str>,
    pub json: bool,
}

pub fn cmd_add_pack(repo_root: &Path, options: AddPackOptions<'_>) -> Result<()> {
    let AddPackOptions {
        source,
        targets,
        reference,
        git_ref,
        path,
        plugin,
        json,
    } = options;
    // Before harness names are checked, so a bundle format gets its own
    // answer rather than "unknown harness".
    canonical_target_ids(
        &targets
            .iter()
            .filter(|target| crate::adapter::bundle_writer(target).is_some())
            .cloned()
            .collect::<Vec<_>>(),
    )?;
    if crate::git::is_git_url(source) {
        if reference.is_some() {
            return Err(TuffError::usage(
                "--reference names the OCI repository an artifact came from; a pack from git records its repository itself",
            ));
        }
        // Checked before anything is cloned.
        lockfile::require_lockfile(repo_root)?;
        let target_ids =
            canonical_target_ids(&resolve_agent_selection(repo_root, targets, false)?)?;
        let checkout = super::pack_git::checkout(super::pack_git::CheckoutRequest {
            url: source,
            path,
            git_ref,
            plugin,
            allow_remote: false,
        })?;
        let (_imported, root) = source_pack_of(&checkout)?;
        let (_built, artifact) = build_pack_for_targets(&root, &target_ids)?;
        let source = PackSource::Git(checkout.source.clone());
        install_pack_artifact(repo_root, &artifact, &target_ids, &source)?;
        return report_pack_install(&artifact, &source, &target_ids, json);
    }
    if git_ref.is_some() || path.is_some() || plugin.is_some() {
        return Err(TuffError::usage(
            "--ref, --path, and --plugin apply to a pack installed from a git address",
        ));
    }
    // Normalized before any install work starts: a malformed --reference
    // should fail closed, not install the pack and then fail on cleanup.
    let registry = reference.map(oci::normalize_pack_repository).transpose()?;
    let artifact = pack::read_artifact(Path::new(source))?;
    let target_ids = canonical_target_ids(&resolve_agent_selection(repo_root, targets, false)?)?;
    let source = PackSource::Artifact(lockfile::PackArtifactSource { registry });
    install_pack_artifact(repo_root, &artifact, &target_ids, &source)?;
    report_pack_install(&artifact, &source, &target_ids, json)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PackInstallReport<'a> {
    schema: u8,
    name: &'a str,
    version: &'a str,
    digest: &'a str,
    source: &'a PackSource,
    targets: &'a [String],
    members: Vec<PackInstallMember<'a>>,
    /// What an imported source had that the pack does not carry.
    unsupported: &'a [pack::PackUnsupported],
}

#[derive(serde::Serialize)]
struct PackInstallMember<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    capability_type: crate::manifest::CapabilityType,
    version: &'a str,
}

fn report_pack_install(
    artifact: &PackArtifact,
    source: &PackSource,
    targets: &[String],
    json: bool,
) -> Result<()> {
    if json {
        let report = PackInstallReport {
            schema: 1,
            name: &artifact.metadata.name,
            version: &artifact.metadata.version,
            digest: &artifact.digest,
            source,
            targets,
            members: artifact
                .metadata
                .capabilities
                .iter()
                .map(|capability| PackInstallMember {
                    id: &capability.id,
                    capability_type: capability.capability_type,
                    version: &capability.version,
                })
                .collect(),
            unsupported: unsupported_of(artifact.metadata.import.as_ref()),
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    match source {
        PackSource::Git(git) => println!(
            "installed pack {} {} from {} ({})",
            artifact.metadata.name,
            artifact.metadata.version,
            git.url,
            describe_git_ref(git)
        ),
        PackSource::Artifact(_) => println!(
            "installed pack {} {} (sha256:{})",
            artifact.metadata.name, artifact.metadata.version, artifact.digest
        ),
    }
    for capability in &artifact.metadata.capabilities {
        println!("  - {} {}", capability.id, capability.version);
    }
    super::pack_import::print_unsupported(unsupported_of(artifact.metadata.import.as_ref()));
    if let PackSource::Git(git) = source
        && git.tag.is_none()
        && git.branch.is_some()
    {
        println!(
            "note: the repository publishes no release tags, so this install follows the branch; tuff update moves it to the branch's newest commit"
        );
    }
    Ok(())
}

/// `tag v1.2.0 at 4b1e9c2`, `branch main at 4b1e9c2`, or `commit 4b1e9c2`.
fn describe_git_ref(git: &lockfile::PackGitSource) -> String {
    let commit = super::short_sha(&git.git_ref);
    match (&git.tag, &git.branch) {
        (Some(tag), _) => format!("tag {tag} at {commit}"),
        (None, Some(branch)) => format!("branch {branch} at {commit}"),
        (None, None) => format!("commit {commit}"),
    }
}

/// Install every member of a verified artifact into the project as one
/// staged change, recording `source` as where the pack came from.
fn install_pack_artifact(
    repo_root: &Path,
    artifact: &PackArtifact,
    target_ids: &[String],
    source: &PackSource,
) -> Result<()> {
    let current_lock = lockfile::require_lockfile(repo_root)?;
    if !current_lock
        .pack_members(&artifact.metadata.name)
        .is_empty()
    {
        return Err(TuffError::refused(format!(
            "pack {} is already installed",
            artifact.metadata.name
        ))
        .with_hint("use 'tuff update <member>' to move it to another release"));
    }
    preflight_pack_install(repo_root, artifact, &current_lock, target_ids)?;
    let staging = tempfile::tempdir()?;
    lockfile::write_lockfile_at(&staging.path().join("tuff.lock"), &current_lock)?;
    copy_shared_configuration(repo_root, staging.path(), target_ids)?;
    let sources = staging.path().join("sources");
    pack::extract_prefix(artifact, "sources", &sources)?;

    for capability in &artifact.metadata.capabilities {
        let manifest = manifest::load_manifest(&sources.join(&capability.id))?;
        validate_artifact_member(capability, &manifest)?;
        for note in manifest::install_notes(&manifest) {
            eprintln!("note: {note}");
        }
        let resolved = resolve_capability(&manifest)?;
        install_capability(
            staging.path(),
            Scope::Project,
            &resolved,
            &manifest,
            target_ids,
            None,
            false,
        )?;
    }

    validate_staged_target_hashes(staging.path(), artifact, target_ids)?;
    let mut staged_lock = lockfile::read_lockfile_at(&staging.path().join("tuff.lock"))?;
    record_pack_entry(&mut staged_lock, artifact, source);
    let provenance = PackProvenance {
        name: artifact.metadata.name.clone(),
        version: artifact.metadata.version.clone(),
        digest: artifact.digest.clone(),
        registry: source.registry().map(str::to_string),
        path: String::new(),
    };
    for capability in &artifact.metadata.capabilities {
        let entry = staged_lock
            .capabilities
            .get_mut(&capability.id)
            .ok_or_else(|| {
                TuffError::new(format!(
                    "staged pack installation did not track '{}'",
                    capability.id
                ))
            })?;
        entry.source = lockfile::CapabilitySource::Pack(PackProvenance {
            path: capability.source_path.clone(),
            ..provenance.clone()
        });
        entry.version_scheme = entry.source.version_scheme_for(&entry.version);
    }
    lockfile::write_lockfile_at(&staging.path().join("tuff.lock"), &staged_lock)?;

    let mutations = collect_install_mutations(
        repo_root,
        staging.path(),
        artifact,
        &staged_lock,
        target_ids,
    )?;
    commit_mutations(&mutations)?;
    Ok(())
}

/// Record the pack itself in the lockfile's packs section (RFC-110 D2).
fn record_pack_entry(lock: &mut lockfile::Lockfile, artifact: &PackArtifact, source: &PackSource) {
    lock.packs.insert(
        artifact.metadata.name.clone(),
        PackLockEntry {
            version: artifact.metadata.version.clone(),
            digest: artifact.digest.clone(),
            source: source.clone(),
            x: artifact.metadata.x.clone(),
        },
    );
}

fn preflight_pack_install(
    repo_root: &Path,
    artifact: &PackArtifact,
    lock: &lockfile::Lockfile,
    target_ids: &[String],
) -> Result<()> {
    for capability in &artifact.metadata.capabilities {
        if lock.capabilities.contains_key(&capability.id) {
            return Err(TuffError::refused(format!(
                "capability '{}' is already tracked",
                capability.id
            ))
            .with_hint(
                "pack installation is all-or-nothing; delete it first or choose another pack",
            ));
        }
    }
    for target_id in target_ids {
        let target = artifact_target(artifact, target_id)?;
        for capability in &target.capabilities {
            for path in &capability.emitted_files {
                let destination = repo_root.join(path);
                if destination.exists() {
                    return Err(TuffError::refused(format!(
                        "refusing to overwrite untracked file at {}",
                        destination.display()
                    ))
                    .with_hint("pack installation is all-or-nothing; move the file aside first"));
                }
            }
        }
    }
    Ok(())
}

fn copy_shared_configuration(source: &Path, destination: &Path, targets: &[String]) -> Result<()> {
    // The capability-index regeneration triggered by `install_capability`
    // reads `registered_adapters`, which reads this file's `agents` list.
    // Without it staged in advance, `read_config_at` would fall back to
    // writing a stray *default* config into the staging tree (debt #18)
    // and the index would regenerate against zero configured agents.
    let project_config = crate::paths::project_config(source);
    if project_config.is_file() {
        fs::copy(&project_config, crate::paths::project_config(destination))?;
    }

    for target in targets {
        let adapter = AdapterKind::from_id(target)
            .ok_or_else(|| TuffError::usage(format!("unknown harness '{target}'")))?;
        for relative in [
            adapter.hook_settings_relpath(),
            adapter.mcp_config_relpath(),
        ] {
            let source_file = source.join(relative);
            if source_file.is_file() {
                let destination_file = destination.join(relative);
                if let Some(parent) = destination_file.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(source_file, destination_file)?;
            }
        }
    }
    Ok(())
}

fn validate_artifact_member(
    expected: &PackArtifactCapability,
    manifest: &manifest::CapabilityManifest,
) -> Result<()> {
    if manifest.id != expected.id
        || manifest.version != expected.version
        || manifest.capability_type != expected.capability_type
    {
        return Err(TuffError::corrupt(format!(
            "artifact source metadata does not match capability '{}'",
            expected.id
        )));
    }
    Ok(())
}

fn validate_staged_target_hashes(
    stage: &Path,
    artifact: &PackArtifact,
    targets: &[String],
) -> Result<()> {
    for target in targets {
        for capability in &artifact_target(artifact, target)?.capabilities {
            let actual = crate::cache::hash_tree(&stage.join(&capability.installed_path))?;
            if actual != capability.sha256 {
                return Err(TuffError::corrupt(format!(
                    "rendered target hash mismatch for '{}' on '{}'",
                    capability.id, target
                )));
            }
        }
    }
    Ok(())
}

struct FileMutation {
    path: PathBuf,
    bytes: Vec<u8>,
    must_be_absent: bool,
}

fn collect_install_mutations(
    repo_root: &Path,
    stage: &Path,
    artifact: &PackArtifact,
    staged_lock: &lockfile::Lockfile,
    targets: &[String],
) -> Result<Vec<FileMutation>> {
    let mut paths = BTreeMap::<PathBuf, bool>::new();
    for target in targets {
        let adapter = AdapterKind::from_id(target)
            .ok_or_else(|| TuffError::usage(format!("unknown harness '{target}'")))?;
        for capability in &artifact.metadata.capabilities {
            let target_entry = staged_lock
                .capabilities
                .get(&capability.id)
                .and_then(|entry| entry.targets.get(adapter.id()))
                .ok_or_else(|| {
                    TuffError::new(format!(
                        "staged lockfile is missing '{}' for target '{}'",
                        capability.id,
                        adapter.id()
                    ))
                })?;
            collect_mutation_tree(stage, &stage.join(&target_entry.installed_path), &mut paths)?;
            for hook in &target_entry.managed_hooks {
                paths.insert(PathBuf::from(&hook.settings_path), false);
            }
        }

        // Installing this pack's capabilities regenerates the capability-
        // index skill inside the staging tree (see `install_capability`).
        // It isn't one of the pack's own declared capabilities, so the loop
        // above never walks it — without this, the staged lockfile would
        // reference index files that never made it to `repo_root`.
        if let Some(index_entry) = staged_lock
            .capabilities
            .get(capability_index::CAPABILITY_INDEX_ID)
            && let Some(target_entry) = index_entry.targets.get(adapter.id())
        {
            collect_mutation_tree(stage, &stage.join(&target_entry.installed_path), &mut paths)?;
            // A project that already has a tool or MCP server
            // already has an index on disk, and it is tracked. The pack's
            // regenerated index replaces it; refusing to overwrite it would
            // make every such project unable to install a pack at all.
            let index_tracked_here = lockfile::require_lockfile(repo_root)
                .ok()
                .and_then(|current| {
                    current
                        .capabilities
                        .get(capability_index::CAPABILITY_INDEX_ID)
                        .map(|entry| entry.targets.contains_key(adapter.id()))
                })
                .unwrap_or(false);
            if index_tracked_here {
                let index_root = PathBuf::from(&target_entry.installed_path);
                for (path, must_be_absent) in paths.iter_mut() {
                    if path.starts_with(&index_root) {
                        *must_be_absent = false;
                    }
                }
            }
        }

        let mcp_path = PathBuf::from(adapter.mcp_config_relpath());
        if stage.join(&mcp_path).is_file() {
            paths.insert(mcp_path, false);
        }
    }
    paths.insert(PathBuf::from("tuff.lock"), false);

    paths
        .into_iter()
        .map(|(relative, must_be_absent)| {
            let source = stage.join(&relative);
            Ok(FileMutation {
                path: repo_root.join(relative),
                bytes: fs::read(&source).map_err(|error| {
                    TuffError::new(format!(
                        "staged pack output is missing {}: {error}",
                        source.display()
                    ))
                })?,
                must_be_absent,
            })
        })
        .collect()
}

fn collect_mutation_tree(
    stage: &Path,
    current: &Path,
    paths: &mut BTreeMap<PathBuf, bool>,
) -> Result<()> {
    let mut entries = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(TuffError::refused(format!(
                "symbolic links are not allowed in staged pack output: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            collect_mutation_tree(stage, &path, paths)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(stage)
                .map_err(|error| TuffError::of(crate::error::ErrorKind::Io, error.to_string()))?;
            paths.insert(relative.to_path_buf(), true);
        }
    }
    Ok(())
}

fn commit_mutations(mutations: &[FileMutation]) -> Result<()> {
    let mut created_directories = BTreeSet::new();
    for mutation in mutations {
        if mutation.must_be_absent && mutation.path.exists() {
            return Err(TuffError::refused(format!(
                "refusing to overwrite untracked file at {}",
                mutation.path.display()
            )));
        }
        if mutation.path.exists() && !mutation.path.is_file() {
            return Err(TuffError::refused(format!(
                "pack installation target is not a regular file: {}",
                mutation.path.display()
            )));
        }
        let mut parent = mutation.path.parent();
        while let Some(directory) = parent {
            if directory.exists() {
                break;
            }
            created_directories.insert(directory.to_path_buf());
            parent = directory.parent();
        }
    }
    let backups = mutations
        .iter()
        .map(|mutation| {
            let previous = if mutation.path.is_file() {
                Some(fs::read(&mutation.path)?)
            } else {
                None
            };
            Ok((mutation.path.clone(), previous))
        })
        .collect::<Result<Vec<_>>>()?;

    for mutation in mutations {
        if let Err(error) = atomic_write(&mutation.path, &mutation.bytes) {
            if let Err(rollback_error) = rollback_mutations(&backups, &created_directories) {
                return Err(TuffError::of(
                    crate::error::ErrorKind::Io,
                    format!(
                        "pack installation failed while writing {}; rollback also failed: {rollback_error}",
                        mutation.path.display()
                    ),
                ));
            }
            return Err(TuffError::of(
                crate::error::ErrorKind::Io,
                format!(
                    "pack installation failed while writing {}: {error}",
                    mutation.path.display()
                ),
            ));
        }
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| TuffError::usage("pack output path has no parent"))?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::Builder::new()
        .prefix("tuff-install-")
        .tempfile_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.flush()?;
    temporary
        .persist(path)
        .map_err(|error| TuffError::of(crate::error::ErrorKind::Io, error.error.to_string()))?;
    Ok(())
}

fn rollback_mutations(
    backups: &[(PathBuf, Option<Vec<u8>>)],
    created_directories: &BTreeSet<PathBuf>,
) -> Result<()> {
    let mut first_error = None;
    for (path, previous) in backups.iter().rev() {
        let result = match previous {
            Some(bytes) => atomic_write(path, bytes),
            None => match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                    ) =>
                {
                    Ok(())
                }
                Err(error) => Err(error.into()),
            },
        };
        if let Err(error) = result
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    let mut directories = created_directories.iter().collect::<Vec<_>>();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        match fs::remove_dir(directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error.into());
                }
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn default_artifact_path(loaded: &LoadedPack) -> PathBuf {
    let leaf = loaded
        .manifest
        .name
        .rsplit('/')
        .next()
        .unwrap_or(&loaded.manifest.name);
    loaded
        .root
        .join(format!("{leaf}-{}.tuffpack", loaded.manifest.version))
}

// ── pack updates ─────────────────────────────────────────────────────

/// A pack-installed capability moves forward with its pack, never alone.
///
/// The pack is the unit of versioning, verification, and preflight: the
/// artifact's target hashes are checked as a whole, and every member records
/// the same `PackProvenance`. Updating one member to 1.2.0 while its siblings
/// stayed at 1.0.0 would leave the lockfile claiming two releases of one pack
/// and no artifact that reproduces either. So `tuff update <member>` resolves
/// the pack every member came from and moves all of them together: members
/// dropped by the new release are removed, new members are installed, and
/// the rest are replaced.
pub struct PackUpdateRequest<'a> {
    pub repo_root: &'a Path,
    pub id: &'a str,
    pub requested_targets: &'a [String],
    pub check: bool,
    pub force: bool,
    /// Update from this artifact instead of resolving the registry. The
    /// offline counterpart of `tuff add pack <file>`: a pulled artifact can
    /// be applied without the registry being reachable at update time.
    pub artifact: Option<&'a Path>,
    pub oci_options: &'a OciTransferOptions,
    pub json: bool,
}

/// Where the new release came from, for the summary line.
enum PackReleaseSource {
    Registry(String),
    Artifact(PathBuf),
    Git(String),
}

pub fn cmd_update_pack(request: PackUpdateRequest<'_>) -> Result<()> {
    let PackUpdateRequest {
        repo_root,
        id,
        requested_targets,
        check,
        force,
        artifact,
        oci_options,
        json,
    } = request;
    let lock = lockfile::require_lockfile(repo_root)?;
    let entry = lock
        .capabilities
        .get(id)
        .ok_or_else(|| TuffError::not_found(format!("'{id}' is not installed")))?;
    let provenance = entry
        .source
        .as_pack()
        .cloned()
        .ok_or_else(|| TuffError::usage(format!("'{id}' was not installed from a pack")))?;

    let members = lock.pack_members(&provenance.name);
    let target_ids = pack_update_targets(&lock, &members, requested_targets)?;
    let installed_source = lock
        .normalized_packs()
        .get(&provenance.name)
        .map(|pack| pack.source.clone())
        // `normalized_packs` describes every pack a member names.
        .unwrap_or_else(|| {
            PackSource::Artifact(lockfile::PackArtifactSource {
                registry: provenance.registry.clone(),
            })
        });

    // The artifact and where it came from. `_built` keeps a pulled or built
    // artifact's directory alive until the update is applied.
    let mut _built = None;
    let (artifact, source, new_source) = match (artifact, &installed_source) {
        (Some(path), _) => {
            let artifact = pack::read_artifact(path)?;
            (
                artifact,
                PackReleaseSource::Artifact(path.to_path_buf()),
                PackSource::Artifact(lockfile::PackArtifactSource {
                    registry: provenance.registry.clone(),
                }),
            )
        }
        (None, PackSource::Git(git)) => {
            let Some(reference) = git_update_ref(id, &provenance, git, check, force, json)? else {
                return Ok(());
            };
            let checkout = super::pack_git::checkout(super::pack_git::CheckoutRequest {
                url: &git.url,
                path: Some(&git.path),
                git_ref: Some(&reference),
                plugin: git.plugin.as_deref(),
                allow_remote: false,
            })?;
            let (_imported, root) = source_pack_of(&checkout)?;
            let (directory, artifact) = build_pack_for_targets(&root, &target_ids)?;
            _built = Some(directory);
            let describe = format!(
                "{} ({})",
                checkout.source.url,
                describe_git_ref(&checkout.source)
            );
            (
                artifact,
                PackReleaseSource::Git(describe),
                PackSource::Git(checkout.source.clone()),
            )
        }
        (None, PackSource::Artifact(_)) => {
            let Some(registry) = provenance.registry.as_deref() else {
                return Err(TuffError::usage(format!(
                    "pack {} was installed without --reference, so there is no registry to check",
                    provenance.name
                ))
                .with_hint(
                    "reinstall with 'tuff add pack <artifact> --reference <registry/repository:tag>', or pass --pack <artifact> to update from a pulled file",
                ));
            };
            let tags = block_on_oci(oci::list_pack_versions(registry, oci_options))?;
            let latest = match super::outdated::compare_pack_versions(&provenance.version, &tags) {
                super::outdated::PackVersionStatus::Current => {
                    // Newest tag or not, the tag may no longer be the bytes
                    // that were installed. Say so rather than "up to date".
                    match super::outdated::verify_pack_tag(&provenance, registry, oci_options) {
                        super::outdated::PackTagIntegrity::Matches => {
                            return print_no_update(
                                &provenance,
                                "current",
                                &format!(
                                    "pack {} is already up to date ({})",
                                    provenance.name, provenance.version
                                ),
                                json,
                            );
                        }
                        super::outdated::PackTagIntegrity::Repointed { live_digest } => {
                            if check {
                                return print_no_update(
                                    &provenance,
                                    "repointed",
                                    &format!(
                                        "pack {} {} was republished: installed sha256:{}, {registry}:{} now serves {live_digest}; 'tuff update {id} --force' replaces the installed release with what the tag serves now",
                                        provenance.name,
                                        provenance.version,
                                        provenance.digest,
                                        provenance.version
                                    ),
                                    json,
                                );
                            }
                            if !force {
                                return Err(TuffError::refused(format!(
                                    "pack {} {} was republished: installed sha256:{}, {registry}:{} now serves {live_digest}",
                                    provenance.name,
                                    provenance.version,
                                    provenance.digest,
                                    provenance.version
                                ))
                                .with_hint(
                                    "inspect with 'tuff outdated', then use --force to replace the installed release with what the tag serves now",
                                ));
                            }
                            provenance.version.clone()
                        }
                        super::outdated::PackTagIntegrity::Missing => {
                            return Err(TuffError::not_found(format!(
                                "pack {} {} is installed but {registry}:{} no longer exists in the registry; the installed release cannot be reproduced from there",
                                provenance.name, provenance.version, provenance.version
                            )));
                        }
                        super::outdated::PackTagIntegrity::Unavailable(error) => {
                            return Err(TuffError::source_failed(format!(
                                "could not verify pack {} {} against {registry}: {error}",
                                provenance.name, provenance.version
                            )));
                        }
                    }
                }
                super::outdated::PackVersionStatus::Unknown => {
                    return Err(TuffError::source_failed(format!(
                        "cannot tell whether pack {} {} is current: only semver tags are compared, \
                         and {registry} publishes none that parse",
                        provenance.name, provenance.version
                    ))
                    .with_hint("pass --pack <artifact> to update from a specific file"));
                }
                super::outdated::PackVersionStatus::Newer(latest) => latest,
            };
            if check && !json {
                // A dry run answers "what would change" without a pull:
                // the version and the membership come from the lockfile
                // and the tag list, the dirtiness from disk. The JSON
                // preview pulls, since it lists the files that change.
                return report_pack_update_plan(
                    repo_root,
                    &provenance,
                    &latest,
                    &members,
                    None,
                    &target_ids,
                );
            }
            let pulled = tempfile::tempdir()?;
            let output = pulled.path().join("pack.tuffpack");
            let reference = format!("{registry}:{latest}");
            block_on_oci(oci::pull_pack(&reference, &output, oci_options))?;
            let artifact = pack::read_artifact(&output)?;
            _built = Some(pulled);
            (
                artifact,
                PackReleaseSource::Registry(reference),
                installed_source.clone(),
            )
        }
    };

    if artifact.metadata.name != provenance.name {
        return Err(TuffError::refused(format!(
            "refusing to update pack {} from an artifact for pack {}",
            provenance.name, artifact.metadata.name
        )));
    }
    // A pack built from git is rebuilt for every update, and its digest
    // changes whenever Tuff renders differently, so for git the commit
    // decides (`git_update_ref`) and this comparison applies to artifacts.
    if artifact.metadata.version == provenance.version
        && matches!(new_source, PackSource::Artifact(_))
    {
        if artifact.digest == provenance.digest {
            return print_no_update(
                &provenance,
                "current",
                &format!(
                    "pack {} is already up to date ({})",
                    provenance.name, provenance.version
                ),
                json,
            );
        }
        if !force {
            return Err(TuffError::refused(format!(
                "pack {} {} is installed from sha256:{} but the artifact with the same version \
                 has sha256:{}; the version was republished with different content",
                provenance.name, provenance.version, provenance.digest, artifact.digest
            ))
            .with_hint("use --force to replace it"));
        }
    }
    for target in &target_ids {
        artifact_target(&artifact, target)?;
    }

    if check && json {
        let preview = tuff_packs::preview::preview_update(
            repo_root,
            &lock,
            &provenance,
            &members,
            &artifact,
            &new_source,
            &target_ids,
        )?;
        println!("{}", serde_json::to_string_pretty(&preview)?);
        return Ok(());
    }
    if check {
        return report_pack_update_plan(
            repo_root,
            &provenance,
            &artifact.metadata.version,
            &members,
            Some(&artifact),
            &target_ids,
        );
    }

    let dirty = dirty_pack_members(repo_root, &lock, &members, &target_ids);
    if !dirty.is_empty() && !force {
        return Err(TuffError::drift(format!(
            "pack {} has local changes in {}",
            provenance.name,
            dirty.join(", ")
        ))
        .with_hint("run 'tuff diff <id>' first, or use --force to replace them"));
    }

    apply_pack_update(
        repo_root,
        &lock,
        &members,
        Some((&artifact, &new_source)),
        &target_ids,
    )?;

    let from = match &source {
        PackReleaseSource::Registry(reference) => reference.clone(),
        PackReleaseSource::Artifact(path) => path.display().to_string(),
        PackReleaseSource::Git(describe) => describe.clone(),
    };
    println!(
        "updated pack {} {} -> {} (sha256:{}) from {from}",
        provenance.name, provenance.version, artifact.metadata.version, artifact.digest
    );
    let new_ids = artifact
        .metadata
        .capabilities
        .iter()
        .map(|capability| capability.id.as_str())
        .collect::<BTreeSet<_>>();
    for capability in &artifact.metadata.capabilities {
        let marker = if members.iter().any(|member| member == &capability.id) {
            "updated"
        } else {
            "added"
        };
        println!("  {marker} {} {}", capability.id, capability.version);
    }
    for member in &members {
        if !new_ids.contains(member.as_str()) {
            println!("  removed {member}");
        }
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct PackListing<'a> {
    schema: u8,
    packs: Vec<PackListEntry<'a>>,
}

#[derive(serde::Serialize)]
struct PackListEntry<'a> {
    name: String,
    version: String,
    digest: String,
    source: PackSource,
    members: Vec<PackListMember<'a>>,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    x: tuff_core::host::HostTablesJson,
}

#[derive(serde::Serialize)]
struct PackListMember<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    capability_type: crate::manifest::CapabilityType,
    version: &'a str,
    targets: Vec<&'a str>,
}

/// `tuff pack list`: every installed pack, where it came from, and its
/// members (RFC-110 D2).
pub fn cmd_pack_list(repo_root: &Path, json: bool) -> Result<()> {
    let lock = lockfile::require_lockfile(repo_root)?;
    let packs = lock
        .normalized_packs()
        .into_iter()
        .map(|(name, pack)| {
            let members = lock
                .pack_members(&name)
                .into_iter()
                .filter_map(|id| lock.capabilities.get_key_value(&id))
                .map(|(id, entry)| PackListMember {
                    id,
                    capability_type: entry.capability_type,
                    version: &entry.version,
                    targets: entry.targets.keys().map(String::as_str).collect(),
                })
                .collect();
            PackListEntry {
                name,
                version: pack.version,
                digest: pack.digest,
                source: pack.source,
                members,
                x: pack.x,
            }
        })
        .collect::<Vec<_>>();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&PackListing { schema: 1, packs })?
        );
        return Ok(());
    }
    if packs.is_empty() {
        println!("no packs installed");
        return Ok(());
    }
    for pack in &packs {
        let source = match &pack.source {
            PackSource::Git(git) => format!("{} ({})", git.url, describe_git_ref(git)),
            PackSource::Artifact(artifact) => match &artifact.registry {
                Some(registry) => format!("{registry} (sha256:{})", super::short_sha(&pack.digest)),
                None => format!("artifact (sha256:{})", super::short_sha(&pack.digest)),
            },
        };
        println!("{} {}  {source}", pack.name, pack.version);
        for member in &pack.members {
            println!(
                "  - {} {} ({}; {})",
                member.id,
                member.version,
                member.capability_type,
                member.targets.join(", ")
            );
        }
    }
    Ok(())
}

/// `tuff pack remove <name>`: delete every member of an installed pack as
/// one staged change, the way `tuff delete` deletes one capability.
pub fn cmd_pack_remove(repo_root: &Path, name: &str, force: bool) -> Result<()> {
    let lock = lockfile::require_lockfile(repo_root)?;
    let members = lock.pack_members(name);
    if members.is_empty() {
        return Err(
            TuffError::not_found(format!("pack {name} is not installed"))
                .with_hint("run 'tuff pack list' to see installed packs"),
        );
    }
    let target_ids = members
        .iter()
        .filter_map(|member| lock.capabilities.get(member))
        .flat_map(|entry| entry.targets.keys().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    for member in &members {
        for target in &target_ids {
            let policies = super::delete::policies_on_server_table(&lock, member, target);
            if let Some(policy) = policies.first() {
                return Err(TuffError::refused(format!(
                    "'{member}' has rules from policy '{policy}' on its table, so pack {name} was not removed"
                ))
                .with_hint(format!(
                    "delete the policy first ('tuff delete {policy}'), or drop its rules for '{member}'"
                )));
            }
        }
    }
    let dirty = dirty_pack_members(repo_root, &lock, &members, &target_ids);
    if !dirty.is_empty() && !force {
        return Err(TuffError::drift(format!(
            "pack {name} has local changes in {}",
            dirty.join(", ")
        ))
        .with_hint("run 'tuff diff <id>' first, or use --force to remove them"));
    }
    let version = lock
        .capabilities
        .get(&members[0])
        .and_then(|entry| entry.source.as_pack())
        .map(|pack| pack.version.clone())
        .unwrap_or_default();
    apply_pack_update(repo_root, &lock, &members, None, &target_ids)?;
    println!("removed pack {name} {version}");
    for member in &members {
        println!("  - {member}");
    }
    Ok(())
}

/// Say that there is nothing to apply: a sentence, or with `--json` the
/// preview document with no members and the reason in `status`.
fn print_no_update(
    provenance: &PackProvenance,
    status: &str,
    message: &str,
    json: bool,
) -> Result<()> {
    if json {
        let document = serde_json::json!({
            "schema": 1,
            "pack": provenance.name,
            "status": status,
            "message": message,
            "from": { "version": provenance.version, "digest": provenance.digest },
            "to": null,
            "members": [],
            "blockedBy": [],
        });
        println!("{}", serde_json::to_string_pretty(&document)?);
    } else {
        println!("{message}");
    }
    Ok(())
}

/// The ref a git pack update checks out, or `None` when there is nothing
/// to do and the reason has been printed. Mirrors the registry path: a
/// repointed tag is refused without `--force` and reported by `--check`.
fn git_update_ref(
    id: &str,
    provenance: &PackProvenance,
    git: &lockfile::PackGitSource,
    check: bool,
    force: bool,
    json: bool,
) -> Result<Option<String>> {
    use super::pack_git::GitPackStatus;
    let (status, _) = super::pack_git::status(&provenance.name, &provenance.version, git)?;
    let installed = describe_git_ref(git);
    match status {
        GitPackStatus::Current => {
            print_no_update(
                provenance,
                "current",
                &format!(
                    "pack {} is already up to date ({}, {installed})",
                    provenance.name, provenance.version
                ),
                json,
            )?;
            Ok(None)
        }
        GitPackStatus::Pinned => Err(TuffError::usage(format!(
            "pack {} was installed from {installed}, so there is no tag or branch to follow",
            provenance.name
        ))
        .with_hint(
            "reinstall with 'tuff add pack <url> --ref <tag>', or pass --pack <artifact> to update from a file",
        )),
        GitPackStatus::TagMissing => Err(TuffError::not_found(format!(
            "pack {} {} is installed from {installed}, but {} no longer has that tag",
            provenance.name, provenance.version, git.url
        ))),
        GitPackStatus::Repointed { live_commit } => {
            let tag = git.tag.clone().unwrap_or_default();
            let message = format!(
                "pack {} {}: tag {tag} now names commit {} in {}, and {} was installed",
                provenance.name,
                provenance.version,
                super::short_sha(&live_commit),
                git.url,
                super::short_sha(&git.git_ref)
            );
            if check {
                print_no_update(
                    provenance,
                    "repointed",
                    &format!("{message}; 'tuff update {id} --force' installs what the tag names now"),
                    json,
                )?;
                return Ok(None);
            }
            if !force {
                return Err(TuffError::refused(message).with_hint(
                    "inspect with 'tuff outdated', then use --force to install what the tag names now",
                ));
            }
            Ok(Some(tag))
        }
        GitPackStatus::Newer { tag, .. } => Ok(Some(tag)),
        GitPackStatus::BranchMoved { .. } => Ok(git.branch.clone()),
    }
}

/// The agents a pack update applies to: every agent the pack is installed
/// for. A narrower `--harness` selection is refused rather than honoured,
/// because it would leave one agent's copy at the old release with a
/// lockfile that can only record one pack version per member.
fn pack_update_targets(
    lock: &lockfile::Lockfile,
    members: &[String],
    requested: &[String],
) -> Result<Vec<String>> {
    let installed = members
        .iter()
        .filter_map(|member| lock.capabilities.get(member))
        .flat_map(|entry| entry.targets.keys().cloned())
        .collect::<BTreeSet<_>>();
    let installed = installed.into_iter().collect::<Vec<_>>();
    if requested.is_empty() {
        return Ok(installed);
    }
    let requested = canonical_target_ids(requested)?;
    let requested_set = requested.iter().collect::<BTreeSet<_>>();
    let installed_set = installed.iter().collect::<BTreeSet<_>>();
    if requested_set != installed_set {
        return Err(TuffError::usage(format!(
            "a pack update applies to every agent the pack is installed for ({})",
            installed.join(", ")
        ))
        .with_hint("drop --harness"));
    }
    Ok(installed)
}

/// Members with local edits on any selected target, as `id (agent)`.
fn report_pack_update_plan(
    repo_root: &Path,
    provenance: &PackProvenance,
    latest: &str,
    members: &[String],
    artifact: Option<&PackArtifact>,
    target_ids: &[String],
) -> Result<()> {
    let lock = lockfile::require_lockfile(repo_root)?;
    println!(
        "pack {} can be updated {} -> {} for {}",
        provenance.name,
        provenance.version,
        latest,
        target_ids.join(", ")
    );
    match artifact {
        Some(artifact) => {
            let new_ids = artifact
                .metadata
                .capabilities
                .iter()
                .map(|capability| capability.id.as_str())
                .collect::<BTreeSet<_>>();
            for capability in &artifact.metadata.capabilities {
                let marker = if members.iter().any(|member| member == &capability.id) {
                    "update"
                } else {
                    "add"
                };
                println!("  {marker} {} {}", capability.id, capability.version);
            }
            for member in members {
                if !new_ids.contains(member.as_str()) {
                    println!("  remove {member}");
                }
            }
        }
        None => {
            // Without the artifact in hand, membership of the new release
            // is unknown; say what is installed rather than guess.
            for member in members {
                println!("  update {member}");
            }
        }
    }
    let dirty = dirty_pack_members(repo_root, &lock, members, target_ids);
    if dirty.is_empty() {
        println!("no local changes; the update would apply cleanly");
    } else {
        println!(
            "local changes in {}; the update would need --force",
            dirty.join(", ")
        );
    }
    Ok(())
}

/// Install a new release on top of a staging tree its predecessor was
/// removed from, the way `tuff add pack` installs, and return the writes
/// that carry the result into the project.
fn stage_release(
    repo_root: &Path,
    stage: &Path,
    staged_lock_path: &Path,
    artifact: &PackArtifact,
    source: &PackSource,
    target_ids: &[String],
) -> Result<Vec<FileMutation>> {
    // Now identical to a fresh `tuff add pack` against the staged state.
    let staged_lock = lockfile::read_lockfile_at(staged_lock_path)?;
    for capability in &artifact.metadata.capabilities {
        if staged_lock.capabilities.contains_key(&capability.id) {
            return Err(TuffError::refused(format!(
                "capability '{}' in pack {} {} is already tracked from another source; pack update is all-or-nothing",
                capability.id, artifact.metadata.name, artifact.metadata.version
            )));
        }
    }
    let sources = stage.join("sources");
    pack::extract_prefix(artifact, "sources", &sources)?;
    for capability in &artifact.metadata.capabilities {
        let manifest = manifest::load_manifest(&sources.join(&capability.id))?;
        validate_artifact_member(capability, &manifest)?;
        for note in manifest::install_notes(&manifest) {
            eprintln!("note: {note}");
        }
        let resolved = resolve_capability(&manifest)?;
        install_capability(
            stage,
            Scope::Project,
            &resolved,
            &manifest,
            target_ids,
            None,
            false,
        )?;
    }
    validate_staged_target_hashes(stage, artifact, target_ids)?;

    let mut staged_lock = lockfile::read_lockfile_at(staged_lock_path)?;
    record_pack_entry(&mut staged_lock, artifact, source);
    let new_provenance = PackProvenance {
        name: artifact.metadata.name.clone(),
        version: artifact.metadata.version.clone(),
        digest: artifact.digest.clone(),
        registry: source.registry().map(str::to_string),
        path: String::new(),
    };
    for capability in &artifact.metadata.capabilities {
        let entry = staged_lock
            .capabilities
            .get_mut(&capability.id)
            .ok_or_else(|| {
                TuffError::new(format!(
                    "staged pack update did not track '{}'",
                    capability.id
                ))
            })?;
        entry.source = lockfile::CapabilitySource::Pack(PackProvenance {
            path: capability.source_path.clone(),
            ..new_provenance.clone()
        });
        entry.version_scheme = entry.source.version_scheme_for(&entry.version);
    }
    lockfile::write_lockfile_at(staged_lock_path, &staged_lock)?;

    collect_install_mutations(repo_root, stage, artifact, &staged_lock, target_ids)
}

/// Replace one pack release with another in a staging tree, then commit
/// the result to the project as one set of file writes plus the removal of
/// whatever the old release emitted and the new one does not.
///
/// The staging tree starts from the project's lockfile and shared
/// configuration, the old members are removed from it exactly as
/// `tuff delete` would remove them (hook registrations and MCP entries
/// included), and the new release is installed on top through the same
/// path as `tuff add pack`. The project itself is untouched until every
/// step has succeeded.
fn apply_pack_update(
    repo_root: &Path,
    lock: &lockfile::Lockfile,
    members: &[String],
    release: Option<(&PackArtifact, &PackSource)>,
    target_ids: &[String],
) -> Result<()> {
    let staging = tempfile::tempdir()?;
    let staged_lock_path = staging.path().join("tuff.lock");
    lockfile::write_lockfile_at(&staged_lock_path, lock)?;
    let mut staged_lock = lockfile::read_lockfile_at(&staged_lock_path)?;

    // Paths the old release owns in the project. Each is either rewritten
    // by the new release (present in staging afterwards) or stale.
    let mut previous_paths = BTreeSet::<PathBuf>::new();
    for member in members {
        let Some(entry) = lock.capabilities.get(member) else {
            continue;
        };
        for target in target_ids {
            let Some(target_entry) = entry.targets.get(target) else {
                continue;
            };
            if !target_entry.installed_path.is_empty() {
                let tree = repo_root.join(&target_entry.installed_path);
                if tree.is_dir() {
                    collect_project_tree(repo_root, &tree, &mut previous_paths)?;
                }
            }
            for hook in &target_entry.managed_hooks {
                previous_paths.insert(PathBuf::from(&hook.settings_path));
            }
            if let Some(mcp_entry) = &target_entry.managed_mcp_entry {
                previous_paths.insert(PathBuf::from(&mcp_entry.config_path));
            }
        }
    }

    // The capability index is derived from the lockfile, so the old
    // release's index is owned by the old release too: when the new one
    // leaves nothing to index, the file must go rather than linger untracked.
    if let Some(index_entry) = lock.capabilities.get(capability_index::CAPABILITY_INDEX_ID) {
        for target in target_ids {
            let Some(target_entry) = index_entry.targets.get(target) else {
                continue;
            };
            let tree = repo_root.join(&target_entry.installed_path);
            if !target_entry.installed_path.is_empty() && tree.is_dir() {
                collect_project_tree(repo_root, &tree, &mut previous_paths)?;
            }
        }
    }

    copy_shared_configuration(repo_root, staging.path(), target_ids)?;
    for member in members {
        let Some(entry) = lock.capabilities.get(member) else {
            continue;
        };
        for target in target_ids {
            let Some(target_entry) = entry.targets.get(target) else {
                continue;
            };
            let adapter = AdapterKind::from_id(target)
                .ok_or_else(|| TuffError::usage(format!("unknown harness '{target}'")))?;
            adapter.remove(member, staging.path(), &target_entry.managed_hooks)?;
        }
        staged_lock.capabilities.remove(member);
    }
    lockfile::write_lockfile_at(&staged_lock_path, &staged_lock)?;

    let mut mutations = match release {
        Some((artifact, source)) => stage_release(
            repo_root,
            staging.path(),
            &staged_lock_path,
            artifact,
            source,
            target_ids,
        )?,
        None => {
            // A removal: the index lists what is left, or goes.
            capability_index::regenerate_capability_index(staging.path(), Scope::Project)?;
            vec![FileMutation {
                path: repo_root.join("tuff.lock"),
                bytes: fs::read(&staged_lock_path)?,
                must_be_absent: false,
            }]
        }
    };
    // Files the old release emitted may legitimately exist in the project;
    // the new release overwrites them. Anything the old release owned that
    // the new one no longer writes is removed after the commit.
    let mut stale = Vec::new();
    for relative in &previous_paths {
        let destination = repo_root.join(relative);
        if let Some(mutation) = mutations
            .iter_mut()
            .find(|mutation| mutation.path == destination)
        {
            mutation.must_be_absent = false;
        } else if staging.path().join(relative).is_file() {
            mutations.push(FileMutation {
                path: destination,
                bytes: fs::read(staging.path().join(relative))?,
                must_be_absent: false,
            });
        } else {
            stale.push(destination);
        }
    }
    commit_mutations(&mutations)?;
    for path in stale {
        if path.is_file() {
            fs::remove_file(&path)?;
        }
        remove_empty_parents(&path, repo_root);
    }
    Ok(())
}

/// Collect every regular file under `current`, relative to `repo_root`.
fn collect_project_tree(
    repo_root: &Path,
    current: &Path,
    paths: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let mut entries = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            collect_project_tree(repo_root, &path, paths)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(repo_root)
                .map_err(|error| TuffError::of(crate::error::ErrorKind::Io, error.to_string()))?;
            paths.insert(relative.to_path_buf());
        }
    }
    Ok(())
}

/// Remove directories left empty by a stale-file removal, stopping at the
/// project root. Best effort: a directory that cannot be removed is left.
fn remove_empty_parents(path: &Path, repo_root: &Path) {
    let mut parent = path.parent();
    while let Some(directory) = parent {
        if directory == repo_root
            || !directory.starts_with(repo_root)
            || fs::read_dir(directory)
                .map(|mut entries| entries.next().is_some())
                .unwrap_or(true)
        {
            break;
        }
        if fs::remove_dir(directory).is_err() {
            break;
        }
        parent = directory.parent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_mutations_restores_previous_files_after_later_write_fails() {
        let temp = tempfile::tempdir().unwrap();
        let existing = temp.path().join("existing");
        let blocking_parent = temp.path().join("blocking-parent");
        fs::write(&existing, "before").unwrap();
        fs::write(&blocking_parent, "not a directory").unwrap();
        let mutations = vec![
            FileMutation {
                path: existing.clone(),
                bytes: b"after".to_vec(),
                must_be_absent: false,
            },
            FileMutation {
                path: blocking_parent.join("child"),
                bytes: b"unreachable".to_vec(),
                must_be_absent: false,
            },
        ];

        let error = commit_mutations(&mutations).unwrap_err();
        eprintln!("{error}");

        assert_eq!(fs::read_to_string(existing).unwrap(), "before");
    }
}
