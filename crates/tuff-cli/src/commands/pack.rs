use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::adapter::{
    AdapterKind, AgentAdapter, CapabilityKind, HookDefinition, HookRenderContext,
    resolve_capability,
};
use crate::error::{Result, TuffError};
use crate::lockfile::{self, PackProvenance};
use crate::manifest;
use crate::oci::{self, OciPushStatus, OciTransferOptions};
use crate::pack::{
    self, LoadedPack, PackArtifact, PackArtifactCapability, PackArtifactContent,
    PackArtifactMetadata, PackArtifactTarget, PackArtifactTargetCapability,
};
use crate::resolver::Scope;

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
        return Err(TuffError::new(
            "--capability, --agent, --version, and --description require --from-project",
        ));
    }
    let manifest_path = root.join(pack::PACK_MANIFEST_FILE);
    if manifest_path.exists() {
        return Err(TuffError::new(format!(
            "refusing to overwrite existing pack manifest: {}",
            manifest_path.display()
        )));
    }
    let manifest = pack::PackManifest {
        schema: pack::PACK_SCHEMA_VERSION,
        name: options.name,
        version: "0.1.0".into(),
        description: "Describe the capabilities and runtime contract in this pack.".into(),
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
            return Err(TuffError::new(
                "a source-pack path cannot be combined with --name",
            ));
        }
        validate_pack_name(name)?;
        let prepared = prepare_one_shot_project_pack(repo_root, &options)?;
        validate_project_source_baselines(&prepared)?;
        let output = options
            .output
            .unwrap_or_else(|| default_project_artifact_path(repo_root, &prepared.loaded.manifest));
        return build_loaded_pack(&prepared.loaded, &output);
    }
    if options.version.is_some()
        || options.description.is_some()
        || !options.capabilities.is_empty()
        || !options.agents.is_empty()
    {
        return Err(TuffError::new(
            "--version, --description, --capability, and --agent require --name for a one-shot project build",
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
        return build_loaded_pack(&prepared.loaded, &output);
    }
    let loaded = pack::load_pack(path)?;
    let output = options
        .output
        .unwrap_or_else(|| default_artifact_path(&loaded));
    build_loaded_pack(&loaded, &output)
}

fn build_loaded_pack(loaded: &LoadedPack, output: &Path) -> Result<()> {
    if output.exists() {
        return Err(TuffError::new(format!(
            "refusing to overwrite existing pack artifact: {}",
            output.display()
        )));
    }
    let (metadata, contents) = render_artifact(loaded)?;
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
        return Err(TuffError::new(format!(
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
        return Err(TuffError::new(
            "this project has no packageable capabilities; add a capability first",
        ));
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
        .ok_or_else(|| TuffError::new("pack manifest path has no parent"))?;
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
        .ok_or_else(|| TuffError::new("project pack name is required"))?;
    let lock = lockfile::require_lockfile(repo_root)?;
    let selected = if options.capabilities.is_empty() {
        default_project_capabilities(&lock)
    } else {
        options.capabilities.clone()
    };
    if selected.is_empty() {
        return Err(TuffError::new(
            "this project has no packageable capabilities; add a capability first",
        ));
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
                return Err(TuffError::new(format!(
                    "source for '{}' no longer reproduces its accepted '{}' baseline; run 'tuff update {}' before building the pack",
                    capability.id, target.id, capability.id
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
        return Err(TuffError::new(
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
        return resolve_agent_selection(root, requested, false);
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

pub fn cmd_pack_inspect(path: &Path, json: bool) -> Result<()> {
    let artifact = pack::read_artifact(path)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&artifact.metadata)?);
        return Ok(());
    }
    println!("pack: {}", artifact.metadata.name);
    println!("version: {}", artifact.metadata.version);
    println!("digest: sha256:{}", artifact.digest);
    println!("capabilities: {}", artifact.metadata.capabilities.len());
    for capability in &artifact.metadata.capabilities {
        println!(
            "  - {} {} ({})",
            capability.id, capability.version, capability.capability_type
        );
    }
    println!("targets: {}", artifact.metadata.targets.len());
    for target in &artifact.metadata.targets {
        println!("  - {}", target.id);
    }
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
        return Err(TuffError::new(format!(
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

pub fn cmd_add_pack(
    repo_root: &Path,
    path: &Path,
    target_ids: &[String],
    reference: Option<&str>,
) -> Result<()> {
    // Normalized before any install work starts: a malformed --reference
    // should fail closed, not install the pack and then fail on cleanup.
    let registry = reference.map(oci::normalize_pack_repository).transpose()?;
    let artifact = pack::read_artifact(path)?;
    let target_ids = canonical_target_ids(&resolve_agent_selection(repo_root, target_ids, false)?)?;
    let current_lock = lockfile::require_lockfile(repo_root)?;
    preflight_pack_install(repo_root, &artifact, &current_lock, &target_ids)?;

    let staging = tempfile::tempdir()?;
    lockfile::write_lockfile_at(&staging.path().join("tuff.lock"), &current_lock)?;
    copy_shared_configuration(repo_root, staging.path(), &target_ids)?;
    let sources = staging.path().join("sources");
    pack::extract_prefix(&artifact, "sources", &sources)?;

    for capability in &artifact.metadata.capabilities {
        let manifest = manifest::load_manifest(&sources.join(&capability.id))?;
        validate_artifact_member(capability, &manifest)?;
        let resolved = resolve_capability(&manifest)?;
        install_capability(
            staging.path(),
            Scope::Project,
            &resolved,
            &manifest,
            &target_ids,
            None,
            false,
        )?;
    }

    validate_staged_target_hashes(staging.path(), &artifact, &target_ids)?;
    let mut staged_lock = lockfile::read_lockfile_at(&staging.path().join("tuff.lock"))?;
    let provenance = PackProvenance {
        name: artifact.metadata.name.clone(),
        version: artifact.metadata.version.clone(),
        digest: artifact.digest.clone(),
        registry: registry.clone(),
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
        entry.version_scheme = entry.source.default_version_scheme();
    }
    lockfile::write_lockfile_at(&staging.path().join("tuff.lock"), &staged_lock)?;

    let mutations = collect_install_mutations(
        repo_root,
        staging.path(),
        &artifact,
        &staged_lock,
        &target_ids,
    )?;
    commit_mutations(&mutations)?;
    println!(
        "installed pack {} {} (sha256:{})",
        artifact.metadata.name, artifact.metadata.version, artifact.digest
    );
    for capability in &artifact.metadata.capabilities {
        println!("  - {} {}", capability.id, capability.version);
    }
    Ok(())
}

fn render_artifact(
    loaded: &LoadedPack,
) -> Result<(PackArtifactMetadata, Vec<PackArtifactContent>)> {
    let mut adapters = validate_pack_targets(loaded)?;
    adapters.sort_by_key(|adapter| adapter.id());
    let capabilities = loaded
        .members
        .iter()
        .map(|member| resolve_capability(&member.manifest))
        .collect::<Result<Vec<_>>>()?;
    let staging = tempfile::tempdir()?;
    let mut targets = Vec::with_capacity(adapters.len());
    let mut contents = pack::source_contents(loaded)?;

    for adapter in adapters {
        let target_root = staging.path().join(adapter.id());
        fs::create_dir_all(&target_root)?;
        let mut target_capabilities = Vec::with_capacity(capabilities.len());
        for capability in &capabilities {
            let planned = adapter.plan(capability, &target_root)?;
            let managed_hooks = managed_hooks_for(capability, adapter, &target_root)?;
            write_render_plan(&target_root, &planned)?;
            register_mcp_if_needed(capability, adapter, &target_root)?;
            let shared_paths = managed_hooks
                .iter()
                .map(|hook| hook.settings_path.as_str())
                .collect::<BTreeSet<_>>();
            let emitted_files = planned
                .iter()
                .filter(|file| !shared_paths.contains(file.path.as_str()))
                .map(|file| file.path.clone())
                .collect();
            let installed_root = target_root
                .join(adapter.dir_prefix())
                .join(capability.capability_type.plural_dir())
                .join(&capability.id);
            target_capabilities.push(PackArtifactTargetCapability {
                id: capability.id.clone(),
                installed_path: lockfile::relative_or_absolute_fs(&installed_root, &target_root),
                sha256: crate::cache::hash_tree(&installed_root)?,
                emitted_files,
                managed_hooks,
            });
        }
        collect_tree_contents(
            &target_root,
            &target_root,
            &format!("targets/{}", adapter.id()),
            &mut contents,
        )?;
        targets.push(PackArtifactTarget {
            id: adapter.id().to_string(),
            capabilities: target_capabilities,
        });
    }

    let capabilities = loaded
        .members
        .iter()
        .map(|member| PackArtifactCapability {
            id: member.manifest.id.clone(),
            capability_type: member.manifest.capability_type,
            version: member.manifest.version.clone(),
            description: member.manifest.description.clone(),
            source_path: member.source_path.clone(),
        })
        .collect();
    let metadata = PackArtifactMetadata {
        artifact_version: pack::PACK_ARTIFACT_VERSION,
        pack_schema: loaded.manifest.schema,
        name: loaded.manifest.name.clone(),
        version: loaded.manifest.version.clone(),
        description: loaded.manifest.description.clone(),
        capabilities,
        targets,
        files: Vec::new(),
    };
    Ok((metadata, contents))
}

fn validate_pack_targets(loaded: &LoadedPack) -> Result<Vec<AdapterKind>> {
    let mut adapters = Vec::with_capacity(loaded.manifest.build.targets.len());
    let mut canonical = BTreeSet::new();
    for target in &loaded.manifest.build.targets {
        let adapter = AdapterKind::from_id(target).ok_or_else(|| {
            TuffError::new(format!(
                "unknown pack build target '{target}'; run 'tuff agent list' to see available agents"
            ))
        })?;
        if !canonical.insert(adapter.id()) {
            return Err(TuffError::new(format!(
                "pack build targets '{}' and another alias resolve to the same adapter '{}'",
                target,
                adapter.id()
            )));
        }
        for member in &loaded.members {
            if !adapter.supports(member.manifest.capability_type) {
                return Err(TuffError::new(format!(
                    "{} does not support {} capability '{}'",
                    adapter.display_name(),
                    member.manifest.capability_type,
                    member.manifest.id
                )));
            }
        }
        adapters.push(adapter);
    }
    Ok(adapters)
}

fn canonical_target_ids(targets: &[String]) -> Result<Vec<String>> {
    let mut canonical = BTreeSet::new();
    for target in targets {
        let adapter = AdapterKind::from_id(target)
            .ok_or_else(|| TuffError::new(format!("unknown agent '{target}'")))?;
        canonical.insert(adapter.id().to_string());
    }
    Ok(canonical.into_iter().collect())
}

fn managed_hooks_for(
    capability: &crate::adapter::ResolvedCapability,
    adapter: AdapterKind,
    root: &Path,
) -> Result<Vec<lockfile::ManagedHook>> {
    let CapabilityKind::Hook { hook } = &capability.kind else {
        return Ok(Vec::new());
    };
    match hook {
        HookDefinition::Command(hook) => Ok(adapter
            .render_standard_hook(HookRenderContext {
                capability_id: &capability.id,
                hook,
                source_files: &capability.source_files,
                repo_root: root,
                track_managed_hooks: true,
            })?
            .managed_hooks),
        HookDefinition::Native(_) => Err(TuffError::new(
            "native hook fragments are not supported in pack manifests",
        )),
    }
}

fn write_render_plan(root: &Path, files: &[crate::adapter::PlannedFile]) -> Result<()> {
    for file in files {
        let destination = root.join(&file.path);
        if destination.exists() && !file.allow_existing {
            return Err(TuffError::new(format!(
                "pack members render the same target path: {}",
                file.path
            )));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(destination, &file.content)?;
    }
    Ok(())
}

fn register_mcp_if_needed(
    capability: &crate::adapter::ResolvedCapability,
    adapter: AdapterKind,
    root: &Path,
) -> Result<()> {
    if let CapabilityKind::McpServer { server } = &capability.kind {
        // `root` is the staging tree here, seeded with the live lockfile and
        // MCP config, so "already tracked" means "this pack is re-installing
        // over its own earlier install" — the one case overwriting is right.
        let tracked = lockfile::require_lockfile(root)
            .map(|lock| super::add::mcp_entry_tracked(&lock, &capability.id, adapter.id()))
            .unwrap_or(false);
        return tuff_core::mcp::register_server(
            &root.join(adapter.mcp_config_relpath()),
            &capability.id,
            adapter.mcp_server_entry(server),
            tracked,
        );
    }
    let CapabilityKind::Tool { implementation, .. } = &capability.kind else {
        return Ok(());
    };
    if !implementation.mcp {
        return Ok(());
    }
    let entrypoint = format!(
        "{}/tools/{}/{}",
        adapter.dir_prefix(),
        capability.id,
        implementation.entrypoint
    );
    tuff_core::mcp::register_tool(
        &root.join(adapter.mcp_config_relpath()),
        &capability.id,
        &implementation.language,
        &[entrypoint],
    )
}

fn collect_tree_contents(
    root: &Path,
    current: &Path,
    prefix: &str,
    output: &mut Vec<PackArtifactContent>,
) -> Result<()> {
    let mut entries = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(TuffError::new(format!(
                "symbolic links are not allowed in rendered pack output: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            collect_tree_contents(root, &path, prefix, output)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| TuffError::new(error.to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            output.push(PackArtifactContent {
                path: format!("{prefix}/{relative}"),
                bytes: fs::read(path)?,
            });
        }
    }
    Ok(())
}

fn preflight_pack_install(
    repo_root: &Path,
    artifact: &PackArtifact,
    lock: &lockfile::Lockfile,
    target_ids: &[String],
) -> Result<()> {
    for capability in &artifact.metadata.capabilities {
        if lock.capabilities.contains_key(&capability.id) {
            return Err(TuffError::new(format!(
                "capability '{}' is already tracked; pack installation is all-or-nothing",
                capability.id
            )));
        }
    }
    for target_id in target_ids {
        let target = artifact_target(artifact, target_id)?;
        for capability in &target.capabilities {
            for path in &capability.emitted_files {
                let destination = repo_root.join(path);
                if destination.exists() {
                    return Err(TuffError::new(format!(
                        "refusing to overwrite untracked file at {}; pack installation is all-or-nothing",
                        destination.display()
                    )));
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
            .ok_or_else(|| TuffError::new(format!("unknown agent '{target}'")))?;
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
        return Err(TuffError::new(format!(
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
                return Err(TuffError::new(format!(
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
            .ok_or_else(|| TuffError::new(format!("unknown agent '{target}'")))?;
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
            // A project that already has a tool, workflow, or MCP server
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
            return Err(TuffError::new(format!(
                "symbolic links are not allowed in staged pack output: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            collect_mutation_tree(stage, &path, paths)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(stage)
                .map_err(|error| TuffError::new(error.to_string()))?;
            paths.insert(relative.to_path_buf(), true);
        }
    }
    Ok(())
}

fn commit_mutations(mutations: &[FileMutation]) -> Result<()> {
    let mut created_directories = BTreeSet::new();
    for mutation in mutations {
        if mutation.must_be_absent && mutation.path.exists() {
            return Err(TuffError::new(format!(
                "refusing to overwrite untracked file at {}",
                mutation.path.display()
            )));
        }
        if mutation.path.exists() && !mutation.path.is_file() {
            return Err(TuffError::new(format!(
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
                return Err(TuffError::new(format!(
                    "pack installation failed while writing {}; rollback also failed: {rollback_error}",
                    mutation.path.display()
                )));
            }
            return Err(TuffError::new(format!(
                "pack installation failed while writing {}: {error}",
                mutation.path.display()
            )));
        }
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| TuffError::new("pack output path has no parent"))?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::Builder::new()
        .prefix("tuff-install-")
        .tempfile_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.flush()?;
    temporary
        .persist(path)
        .map_err(|error| TuffError::new(error.error.to_string()))?;
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

fn artifact_target<'a>(artifact: &'a PackArtifact, target: &str) -> Result<&'a PackArtifactTarget> {
    artifact
        .metadata
        .targets
        .iter()
        .find(|item| item.id == target)
        .ok_or_else(|| {
            TuffError::new(format!(
                "pack '{}' has no target '{}'",
                artifact.metadata.name, target
            ))
        })
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
}

/// Where the new release came from, for the summary line.
enum PackReleaseSource {
    Registry(String),
    Artifact(PathBuf),
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
    } = request;
    let lock = lockfile::require_lockfile(repo_root)?;
    let entry = lock
        .capabilities
        .get(id)
        .ok_or_else(|| TuffError::new(format!("'{id}' is not installed")))?;
    let provenance = entry
        .source
        .as_pack()
        .cloned()
        .ok_or_else(|| TuffError::new(format!("'{id}' was not installed from a pack")))?;

    let members = pack_members(&lock, &provenance.name);
    let target_ids = pack_update_targets(&lock, &members, requested_targets)?;

    let (artifact, source) = match artifact {
        Some(path) => {
            let artifact = pack::read_artifact(path)?;
            (artifact, PackReleaseSource::Artifact(path.to_path_buf()))
        }
        None => {
            let Some(registry) = provenance.registry.as_deref() else {
                return Err(TuffError::new(format!(
                    "pack {} was installed without --reference, so there is no registry to check; \
                     reinstall with 'tuff add pack <artifact> --reference <registry/repository:tag>' \
                     or pass --pack <artifact> to update from a pulled file",
                    provenance.name
                )));
            };
            let tags = block_on_oci(oci::list_pack_versions(registry, oci_options))?;
            let latest = match super::outdated::compare_pack_versions(&provenance.version, &tags) {
                super::outdated::PackVersionStatus::Current => {
                    // Newest tag or not, the tag may no longer be the bytes
                    // that were installed. Say so rather than "up to date".
                    match super::outdated::verify_pack_tag(&provenance, registry, oci_options) {
                        super::outdated::PackTagIntegrity::Matches => {
                            println!(
                                "pack {} is already up to date ({})",
                                provenance.name, provenance.version
                            );
                            return Ok(());
                        }
                        super::outdated::PackTagIntegrity::Repointed { live_digest } => {
                            if check {
                                println!(
                                    "pack {} {} was republished: installed sha256:{}, {registry}:{} now serves {live_digest}; 'tuff update {id} --force' replaces the installed release with what the tag serves now",
                                    provenance.name,
                                    provenance.version,
                                    provenance.digest,
                                    provenance.version
                                );
                                return Ok(());
                            }
                            if !force {
                                return Err(TuffError::new(format!(
                                    "pack {} {} was republished: installed sha256:{}, {registry}:{} now serves {live_digest}; inspect with 'tuff outdated', then use --force to replace the installed release with what the tag serves now",
                                    provenance.name,
                                    provenance.version,
                                    provenance.digest,
                                    provenance.version
                                )));
                            }
                            provenance.version.clone()
                        }
                        super::outdated::PackTagIntegrity::Missing => {
                            return Err(TuffError::new(format!(
                                "pack {} {} is installed but {registry}:{} no longer exists in the registry; the installed release cannot be reproduced from there",
                                provenance.name, provenance.version, provenance.version
                            )));
                        }
                        super::outdated::PackTagIntegrity::Unavailable(error) => {
                            return Err(TuffError::new(format!(
                                "could not verify pack {} {} against {registry}: {error}",
                                provenance.name, provenance.version
                            )));
                        }
                    }
                }
                super::outdated::PackVersionStatus::Unknown => {
                    return Err(TuffError::new(format!(
                        "cannot tell whether pack {} {} is current: only semver tags are compared, \
                         and {registry} publishes none that parse; pass --pack <artifact> to update \
                         from a specific file",
                        provenance.name, provenance.version
                    )));
                }
                super::outdated::PackVersionStatus::Newer(latest) => latest,
            };
            if check {
                // A dry run answers "what would change" without a pull:
                // the version and the membership come from the lockfile
                // and the tag list, the dirtiness from disk.
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
            (artifact, PackReleaseSource::Registry(reference))
        }
    };

    if artifact.metadata.name != provenance.name {
        return Err(TuffError::new(format!(
            "refusing to update pack {} from an artifact for pack {}",
            provenance.name, artifact.metadata.name
        )));
    }
    if artifact.metadata.version == provenance.version {
        if artifact.digest == provenance.digest {
            println!(
                "pack {} is already up to date ({})",
                provenance.name, provenance.version
            );
            return Ok(());
        }
        if !force {
            return Err(TuffError::new(format!(
                "pack {} {} is installed from sha256:{} but the artifact with the same version \
                 has sha256:{}; the version was republished with different content, use --force \
                 to replace it",
                provenance.name, provenance.version, provenance.digest, artifact.digest
            )));
        }
    }
    for target in &target_ids {
        artifact_target(&artifact, target)?;
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
        return Err(TuffError::new(format!(
            "pack {} has local changes in {}; run 'tuff diff <id>' first or use --force to replace them",
            provenance.name,
            dirty.join(", ")
        )));
    }

    apply_pack_update(
        repo_root,
        &lock,
        &provenance,
        &members,
        &artifact,
        &target_ids,
    )?;

    let from = match &source {
        PackReleaseSource::Registry(reference) => reference.clone(),
        PackReleaseSource::Artifact(path) => path.display().to_string(),
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

/// Every capability the lockfile attributes to one pack, sorted by id.
fn pack_members(lock: &lockfile::Lockfile, pack_name: &str) -> Vec<String> {
    lock.capabilities
        .iter()
        .filter(|(_, entry)| {
            entry
                .source
                .as_pack()
                .is_some_and(|pack| pack.name == pack_name)
        })
        .map(|(id, _)| id.clone())
        .collect()
}

/// The agents a pack update applies to: every agent the pack is installed
/// for. A narrower `--agent` selection is refused rather than honoured,
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
        return Err(TuffError::new(format!(
            "a pack update applies to every agent the pack is installed for ({}); drop --agent",
            installed.join(", ")
        )));
    }
    Ok(installed)
}

/// Members with local edits on any selected target, as `id (agent)`.
fn dirty_pack_members(
    repo_root: &Path,
    lock: &lockfile::Lockfile,
    members: &[String],
    target_ids: &[String],
) -> Vec<String> {
    let mut dirty = Vec::new();
    for member in members {
        let Some(entry) = lock.capabilities.get(member) else {
            continue;
        };
        for target in target_ids {
            if let Some(target_entry) = entry.targets.get(target)
                && super::delete::local_modifications(repo_root, member, target_entry).any()
            {
                dirty.push(format!("{member} ({target})"));
            }
        }
    }
    dirty
}

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
    provenance: &PackProvenance,
    members: &[String],
    artifact: &PackArtifact,
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
                .ok_or_else(|| TuffError::new(format!("unknown agent '{target}'")))?;
            adapter.remove(member, staging.path(), &target_entry.managed_hooks)?;
        }
        staged_lock.capabilities.remove(member);
    }
    lockfile::write_lockfile_at(&staged_lock_path, &staged_lock)?;

    // Now identical to a fresh `tuff add pack` against the staged state.
    for capability in &artifact.metadata.capabilities {
        if staged_lock.capabilities.contains_key(&capability.id) {
            return Err(TuffError::new(format!(
                "capability '{}' in pack {} {} is already tracked from another source; pack update is all-or-nothing",
                capability.id, artifact.metadata.name, artifact.metadata.version
            )));
        }
    }
    let sources = staging.path().join("sources");
    pack::extract_prefix(artifact, "sources", &sources)?;
    for capability in &artifact.metadata.capabilities {
        let manifest = manifest::load_manifest(&sources.join(&capability.id))?;
        validate_artifact_member(capability, &manifest)?;
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

    let mut staged_lock = lockfile::read_lockfile_at(&staged_lock_path)?;
    let new_provenance = PackProvenance {
        name: artifact.metadata.name.clone(),
        version: artifact.metadata.version.clone(),
        digest: artifact.digest.clone(),
        registry: provenance.registry.clone(),
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
        entry.version_scheme = entry.source.default_version_scheme();
    }
    lockfile::write_lockfile_at(&staged_lock_path, &staged_lock)?;

    let mut mutations = collect_install_mutations(
        repo_root,
        staging.path(),
        artifact,
        &staged_lock,
        target_ids,
    )?;
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
                .map_err(|error| TuffError::new(error.to_string()))?;
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
