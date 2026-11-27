//! Building a source pack into an artifact: every harness target rendered
//! the way an install writes it, and every bundle format written by its
//! adapter crate (RFC-104, RFC-110 D9).
//!
//! Nothing here prints. Notes and warnings a build produces come back on
//! [`Built`] and [`Rendered`], for the caller to show.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use tempfile::TempDir;

use tuff_core::error::{ErrorKind, Result, TuffError};
use tuff_core::lockfile;
use tuff_core::pack::{
    self, LoadedPack, PackArtifact, PackArtifactCapability, PackArtifactContent,
    PackArtifactMetadata, PackArtifactTarget, PackArtifactTargetCapability,
};

use crate::adapter::{
    self, AdapterKind, AgentAdapter, CapabilityKind, HookDefinition, HookRenderContext,
    resolve_capability,
};

/// An artifact built into a temporary directory, which is deleted when
/// this is dropped, with what the build had to say.
pub struct Built {
    pub directory: TempDir,
    pub artifact: PackArtifact,
    /// Members a bundle format left out, when the build allowed that.
    pub notes: Vec<String>,
    /// Warnings from bundle writers.
    pub warnings: Vec<String>,
}

/// An artifact's metadata and contents, before they are written.
pub struct Rendered {
    pub metadata: PackArtifactMetadata,
    pub contents: Vec<PackArtifactContent>,
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
}

fn write_built(rendered: Rendered) -> Result<Built> {
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("pack.tuffpack");
    pack::write_artifact(&output, rendered.metadata, rendered.contents)?;
    let artifact = pack::read_artifact(&output)?;
    Ok(Built {
        directory,
        artifact,
        notes: rendered.notes,
        warnings: rendered.warnings,
    })
}

/// Build a source pack for every target its manifest declares.
pub fn build_for_declared_targets(pack_root: &Path) -> Result<Built> {
    let (_, manifest) = pack::load_manifest(pack_root)?;
    let mut loaded = pack::load_pack(pack_root)?;
    let mut targets = canonical_target_ids(&harness_targets(&manifest.build.targets))?;
    targets.extend(
        manifest
            .build
            .targets
            .iter()
            .filter(|target| adapter::bundle_writer(target).is_some())
            .cloned(),
    );
    loaded.manifest.build.targets = targets;
    // An inspection describes what each bundle could hold, so a member a
    // bundle cannot take is left out of it rather than refused.
    write_built(render(&loaded, true)?)
}

/// Build the source pack at `pack_root` for exactly `targets`, which may
/// name bundle formats, whatever its own `[build] targets` list.
pub fn build_as(pack_root: &Path, targets: &[String], skip_unsupported: bool) -> Result<Built> {
    let mut loaded = pack::load_pack(pack_root)?;
    loaded.manifest.build.targets = targets.to_vec();
    write_built(render(&loaded, skip_unsupported)?)
}

/// The targets that are harnesses, leaving out bundle formats.
pub fn harness_targets(targets: &[String]) -> Vec<String> {
    targets
        .iter()
        .filter(|target| adapter::bundle_writer(target).is_none())
        .cloned()
        .collect()
}

/// Load the source pack at `pack_root` and build an artifact holding only
/// `targets`, each of which the pack must declare in `[build] targets`.
/// The artifact lives in the returned temporary directory.
pub fn build_for_targets(pack_root: &Path, targets: &[String]) -> Result<Built> {
    let mut loaded = pack::load_pack(pack_root)?;
    let declared = canonical_target_ids(&harness_targets(&loaded.manifest.build.targets))?;
    for target in targets {
        if !declared.contains(target) {
            return Err(TuffError::unsupported(format!(
                "pack {} is built for {}, not for {target}",
                loaded.manifest.name,
                declared.join(", ")
            ))
            .with_hint("choose one of those harnesses with -a"));
        }
    }
    loaded.manifest.build.targets = targets.to_vec();
    write_built(render(&loaded, false)?)
}

/// Render every target of a pack. A member a bundle format cannot hold
/// fails the build unless `skip_unsupported`, which leaves it out of that
/// bundle and says so.
pub fn render(loaded: &LoadedPack, skip_unsupported: bool) -> Result<Rendered> {
    let PackTargets {
        harnesses: mut adapters,
        bundles,
    } = validate_pack_targets(loaded)?;
    adapters.sort_by_key(|adapter| adapter.id());
    let capabilities = loaded
        .members
        .iter()
        .map(|member| resolve_capability(&member.manifest))
        .collect::<Result<Vec<_>>>()?;
    let staging = tempfile::tempdir()?;
    let mut targets = Vec::with_capacity(adapters.len());
    let mut contents = pack::source_contents(loaded)?;
    let mut notes = Vec::new();
    let mut warnings = Vec::new();

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
                sha256: tuff_core::cache::hash_tree(&installed_root)?,
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
            kind: pack::PackTargetKind::Harness,
            capabilities: target_capabilities,
        });
    }

    for writer in bundles {
        let output = writer.write(&loaded.manifest, &capabilities)?;
        if !output.skipped.is_empty() {
            let list = output
                .skipped
                .iter()
                .map(|item| {
                    format!(
                        "{} '{}': {}",
                        item.kind,
                        item.name.as_deref().unwrap_or_default(),
                        item.reason
                    )
                })
                .collect::<Vec<_>>();
            if !skip_unsupported {
                return Err(TuffError::unsupported(format!(
                    "the {} target cannot hold {}",
                    writer.id(),
                    list.join("; ")
                ))
                .with_hint(format!(
                    "pass --skip-unsupported to build the {} without them",
                    writer.id()
                )));
            }
            for item in &list {
                notes.push(format!("{} leaves out {item}", writer.id()));
            }
        }
        warnings.extend(output.warnings.iter().cloned());
        for file in output.files {
            contents.push(PackArtifactContent {
                path: format!("targets/{}/{}", writer.id(), file.path),
                bytes: file.bytes,
            });
        }
        targets.push(PackArtifactTarget {
            id: writer.id().to_string(),
            kind: pack::PackTargetKind::Bundle,
            capabilities: output
                .members
                .into_iter()
                .map(|member| PackArtifactTargetCapability {
                    id: member.id,
                    installed_path: member.path,
                    sha256: lockfile::hash_bytes(&member.content),
                    emitted_files: member.files,
                    managed_hooks: Vec::new(),
                })
                .collect(),
        });
    }
    targets.sort_by(|a, b| a.id.cmp(&b.id));

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
        homepage: loaded.manifest.homepage.clone(),
        license: loaded.manifest.license.clone(),
        keywords: loaded.manifest.keywords.clone(),
        author: loaded.manifest.author.clone(),
        x: tuff_core::host::to_json(&loaded.manifest.x),
        import: loaded.manifest.import.clone(),
        capabilities,
        targets,
        files: Vec::new(),
    };
    contents.push(PackArtifactContent {
        path: pack::ARTIFACT_MANIFEST_PATH.to_string(),
        bytes: pack::manifest_bytes(loaded)?,
    });
    Ok(Rendered {
        metadata,
        contents,
        notes,
        warnings,
    })
}

/// A pack's build targets: harnesses it installs into, and bundle formats
/// it is written as.
pub struct PackTargets {
    pub harnesses: Vec<AdapterKind>,
    pub bundles: Vec<&'static dyn tuff_core::bundle::BundleWriter>,
}

pub fn validate_pack_targets(loaded: &LoadedPack) -> Result<PackTargets> {
    let mut adapters = Vec::with_capacity(loaded.manifest.build.targets.len());
    let mut bundles: Vec<&'static dyn tuff_core::bundle::BundleWriter> = Vec::new();
    let mut canonical = BTreeSet::new();
    for target in &loaded.manifest.build.targets {
        if let Some(writer) = adapter::bundle_writer(target) {
            if !canonical.insert(writer.id()) {
                return Err(TuffError::usage(format!(
                    "duplicate pack build target '{target}'"
                )));
            }
            bundles.push(writer);
            continue;
        }
        let adapter = AdapterKind::from_id(target).ok_or_else(|| {
            TuffError::usage(format!("unknown pack build target '{target}'")).with_hint(format!(
                "run 'tuff harness list' to see available harnesses; bundle formats: {}",
                adapter::BUNDLE_FORMATS.join(", ")
            ))
        })?;
        if !canonical.insert(adapter.id()) {
            return Err(TuffError::usage(format!(
                "pack build targets '{}' and another alias resolve to the same adapter '{}'",
                target,
                adapter.id()
            )));
        }
        for member in &loaded.members {
            if !adapter.supports(member.manifest.capability_type) {
                return Err(TuffError::unsupported(format!(
                    "{} does not support {} capability '{}'",
                    adapter.display_name(),
                    member.manifest.capability_type,
                    member.manifest.id
                )));
            }
        }
        adapters.push(adapter);
    }
    Ok(PackTargets {
        harnesses: adapters,
        bundles,
    })
}

pub fn canonical_target_ids(targets: &[String]) -> Result<Vec<String>> {
    let mut canonical = BTreeSet::new();
    for target in targets {
        if adapter::bundle_writer(target).is_some() {
            return Err(TuffError::usage(format!(
                "{target} is a bundle format, which is extracted, not installed into a project"
            ))
            .with_hint(format!(
                "write it with 'tuff pack extract <artifact> -a {target} --output <dir>'"
            )));
        }
        let adapter = AdapterKind::from_id(target)
            .ok_or_else(|| TuffError::usage(format!("unknown harness '{target}'")))?;
        canonical.insert(adapter.id().to_string());
    }
    Ok(canonical.into_iter().collect())
}

fn managed_hooks_for(
    capability: &adapter::ResolvedCapability,
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
        HookDefinition::Native(_) => Err(TuffError::unsupported(
            "native hook fragments are not supported in pack manifests",
        )),
    }
}

fn write_render_plan(root: &Path, files: &[adapter::PlannedFile]) -> Result<()> {
    for file in files {
        let destination = root.join(contained_planned_path(&file.path)?);
        if destination.exists() && !file.allow_existing {
            return Err(TuffError::usage(format!(
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
    capability: &adapter::ResolvedCapability,
    adapter: AdapterKind,
    root: &Path,
) -> Result<()> {
    if let CapabilityKind::McpServer { server } = &capability.kind {
        // `root` is the staging tree here, seeded with the live lockfile and
        // MCP config, so "already tracked" means "this pack is re-installing
        // over its own earlier install" — the one case overwriting is right.
        let tracked = lockfile::require_lockfile(root)
            .map(|lock| mcp_entry_tracked(&lock, &capability.id, adapter.id()))
            .unwrap_or(false);
        return tuff_core::mcp::register_server(
            &root.join(adapter.mcp_config_relpath()),
            &capability.id,
            adapter.mcp_server_entry_checked(server)?,
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
            return Err(TuffError::refused(format!(
                "symbolic links are not allowed in rendered pack output: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            collect_tree_contents(root, &path, prefix, output)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| TuffError::of(ErrorKind::Io, error.to_string()))?
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

/// Refuse to write a planned file anywhere but under the install root.
///
/// Planners build these paths from manifest content, so this is the last
/// line of defence if one of them ever lets `..` or an absolute path
/// through: nothing is written outside the project, whatever planned it.
pub fn contained_planned_path(path: &str) -> Result<std::path::PathBuf> {
    pack::validate_relative_path(Path::new(path)).map_err(|_| {
        TuffError::refused(format!(
            "refusing to write outside the install directory: '{path}'"
        ))
    })
}

/// Whether the lockfile already records `id` for `target`: the one case in
/// which overwriting an existing `mcpServers.<id>` entry is a re-install of
/// Tuff's own work, and not a change to something a person wrote by hand.
pub fn mcp_entry_tracked(lockfile: &lockfile::Lockfile, id: &str, target: &str) -> bool {
    lockfile
        .capabilities
        .get(id)
        .is_some_and(|entry| entry.targets.contains_key(target))
}
