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
    let targets = resolve_agent_selection(root, &options.agents, false)?;
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
    let targets = resolve_agent_selection(repo_root, &options.agents, false)?;
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

fn block_on_oci<T>(future: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(future)
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

pub fn cmd_add_pack(repo_root: &Path, path: &Path, target_ids: &[String]) -> Result<()> {
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
        entry.source_path.clear();
        entry.source = None;
        entry.pack = Some(provenance.clone());
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
