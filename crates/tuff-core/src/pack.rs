//! Pack manifests and deterministic `.tuffpack` artifacts.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Result, TuffError};
use crate::lockfile::ManagedHook;
use crate::manifest::{self, CapabilityManifest, CapabilityType};

/// Source-manifest schema understood by this Tuff release.
pub const PACK_SCHEMA_VERSION: u8 = 1;
/// Binary artifact format understood by this Tuff release.
pub const PACK_ARTIFACT_VERSION: u8 = 1;
/// Canonical source-pack manifest filename.
pub const PACK_MANIFEST_FILE: &str = "tuff-pack.toml";

const ARTIFACT_MAGIC: &[u8] = b"TUFFPACK\x01";
const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Declarative source definition for a versioned capability pack.
pub struct PackManifest {
    pub schema: u8,
    pub name: String,
    pub version: String,
    pub description: String,
    pub build: PackBuild,
    pub capabilities: Vec<PackMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Adapter targets rendered while building a pack.
pub struct PackBuild {
    pub targets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// A local manifest-backed capability included in a pack.
pub struct PackMember {
    pub path: String,
}

/// A validated pack member and its loaded capability manifest.
pub struct LoadedPackMember {
    pub source_path: String,
    pub manifest: CapabilityManifest,
}

/// A validated source pack rooted at its canonical filesystem directory.
pub struct LoadedPack {
    pub root: PathBuf,
    pub manifest: PackManifest,
    pub members: Vec<LoadedPackMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Canonical metadata stored at the beginning of a pack artifact.
pub struct PackArtifactMetadata {
    pub artifact_version: u8,
    pub pack_schema: u8,
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<PackArtifactCapability>,
    pub targets: Vec<PackArtifactTarget>,
    pub files: Vec<PackArtifactFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Identity and release metadata for one artifact member.
pub struct PackArtifactCapability {
    pub id: String,
    #[serde(rename = "type")]
    pub capability_type: CapabilityType,
    pub version: String,
    pub description: String,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Pre-rendered adapter metadata stored in an artifact.
pub struct PackArtifactTarget {
    pub id: String,
    pub capabilities: Vec<PackArtifactTargetCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Lifecycle metadata for one capability rendered to one target.
pub struct PackArtifactTargetCapability {
    pub id: String,
    pub installed_path: String,
    pub sha256: String,
    pub emitted_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub managed_hooks: Vec<ManagedHook>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Hash and byte length for one stored artifact file.
pub struct PackArtifactFile {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone)]
/// An artifact-relative path paired with its file bytes.
pub struct PackArtifactContent {
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
/// A fully parsed and verified pack artifact.
pub struct PackArtifact {
    pub metadata: PackArtifactMetadata,
    pub contents: Vec<PackArtifactContent>,
    pub digest: String,
}

/// Loads and validates a source pack from a directory or `tuff-pack.toml` path.
///
/// # Errors
///
/// Returns an error for malformed manifests, unsafe or duplicate paths, invalid capability
/// manifests, incomplete workflow dependencies, or workflow cycles.
pub fn load_pack(path: &Path) -> Result<LoadedPack> {
    let root = if path.is_file() {
        if path.file_name().and_then(|name| name.to_str()) != Some(PACK_MANIFEST_FILE) {
            return Err(TuffError::new(format!(
                "pack source file must be named {PACK_MANIFEST_FILE}"
            )));
        }
        path.parent()
            .ok_or_else(|| TuffError::new("pack manifest has no parent directory"))?
            .to_path_buf()
    } else {
        path.to_path_buf()
    };
    let root = root.canonicalize().map_err(|error| {
        TuffError::new(format!(
            "could not resolve pack root {}: {error}",
            root.display()
        ))
    })?;
    let manifest_path = root.join(PACK_MANIFEST_FILE);
    let manifest: PackManifest =
        toml::from_str(&fs::read_to_string(&manifest_path).map_err(|error| {
            TuffError::new(format!(
                "could not read pack manifest {}: {error}",
                manifest_path.display()
            ))
        })?)
        .map_err(|error| TuffError::new(format!("invalid pack manifest TOML: {error}")))?;

    validate_pack_manifest(&manifest)?;
    let mut members = Vec::with_capacity(manifest.capabilities.len());
    let mut ids = BTreeSet::new();
    for member in &manifest.capabilities {
        let relative = validate_relative_path(Path::new(&member.path))?;
        let member_dir = root.join(&relative);
        reject_symlink_path(&root, &relative)?;
        let canonical = member_dir.canonicalize().map_err(|error| {
            TuffError::new(format!(
                "could not resolve pack capability {}: {error}",
                member_dir.display()
            ))
        })?;
        if !canonical.starts_with(&root) || !canonical.is_dir() {
            return Err(TuffError::new(format!(
                "pack capability path must be a directory inside the pack root: {}",
                member.path
            )));
        }
        let capability = manifest::load_manifest(&canonical)?;
        if !ids.insert(capability.id.clone()) {
            return Err(TuffError::new(format!(
                "duplicate capability id '{}' in pack",
                capability.id
            )));
        }
        members.push(LoadedPackMember {
            source_path: normalize_path(&relative),
            manifest: capability,
        });
    }
    members.sort_by(|left, right| left.manifest.id.cmp(&right.manifest.id));
    validate_workflow_closure(&members)?;

    Ok(LoadedPack {
        root,
        manifest,
        members,
    })
}

/// Writes a source pack manifest as deterministic TOML.
///
/// # Errors
///
/// Returns an error when serialization or filesystem writing fails.
pub fn write_manifest(path: &Path, manifest: &PackManifest) -> Result<()> {
    fs::write(path, toml::to_string_pretty(manifest)?)?;
    Ok(())
}

/// Collects the canonical manifest and declared source files for every pack member.
///
/// # Errors
///
/// Returns an error when a source is missing, unsafe, linked, outside the pack, or duplicated.
pub fn source_contents(pack: &LoadedPack) -> Result<Vec<PackArtifactContent>> {
    let mut contents = BTreeMap::new();
    for member in &pack.members {
        let id = &member.manifest.id;
        let manifest_path = member.manifest.root.join("tuff.toml");
        insert_content(
            &mut contents,
            format!("sources/{id}/tuff.toml"),
            read_safe_member_file(&pack.root, &manifest_path)?,
        )?;
        for source_file in member.manifest.source_files()? {
            let relative = source_file
                .strip_prefix(&member.manifest.root)
                .map_err(|_| TuffError::new("capability source file escaped its manifest root"))?;
            let relative = validate_relative_path(relative)?;
            insert_content(
                &mut contents,
                format!("sources/{id}/{}", normalize_path(&relative)),
                read_safe_member_file(&pack.root, &source_file)?,
            )?;
        }
    }
    Ok(contents
        .into_iter()
        .map(|(path, bytes)| PackArtifactContent { path, bytes })
        .collect())
}

/// Writes a deterministic pack artifact and returns its hexadecimal SHA-256 digest.
///
/// # Errors
///
/// Returns an error for unsafe or duplicate artifact paths, oversized metadata, an existing
/// output path, serialization failure, or filesystem failure.
pub fn write_artifact(
    output: &Path,
    mut metadata: PackArtifactMetadata,
    contents: Vec<PackArtifactContent>,
) -> Result<String> {
    if output.exists() {
        return Err(TuffError::new(format!(
            "refusing to overwrite existing pack artifact: {}",
            output.display()
        )));
    }
    let mut ordered = BTreeMap::new();
    for content in contents {
        validate_relative_path(Path::new(&content.path))?;
        if ordered
            .insert(content.path.clone(), content.bytes)
            .is_some()
        {
            return Err(TuffError::new(format!(
                "duplicate pack artifact path: {}",
                content.path
            )));
        }
    }
    metadata.files = ordered
        .iter()
        .map(|(path, bytes)| PackArtifactFile {
            path: path.clone(),
            sha256: sha256(bytes),
            size: bytes.len() as u64,
        })
        .collect();
    let metadata_bytes = serde_json::to_vec(&metadata)?;
    if metadata_bytes.len() > MAX_METADATA_BYTES {
        return Err(TuffError::new("pack artifact metadata is too large"));
    }

    let mut artifact = Vec::with_capacity(
        ARTIFACT_MAGIC.len()
            + 8
            + metadata_bytes.len()
            + ordered.values().map(Vec::len).sum::<usize>(),
    );
    artifact.extend_from_slice(ARTIFACT_MAGIC);
    artifact.extend_from_slice(&(metadata_bytes.len() as u64).to_be_bytes());
    artifact.extend_from_slice(&metadata_bytes);
    for bytes in ordered.values() {
        artifact.extend_from_slice(bytes);
    }
    let digest = sha256(&artifact);

    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::Builder::new()
        .prefix("tuff-pack-")
        .tempfile_in(parent)?;
    temporary.write_all(&artifact)?;
    temporary.flush()?;
    temporary
        .persist(output)
        .map_err(|error| TuffError::new(error.error.to_string()))?;
    Ok(digest)
}

/// Reads and verifies a complete pack artifact.
///
/// # Errors
///
/// Returns an error for unsupported versions, non-canonical metadata, unsafe paths, truncated or
/// trailing data, invalid hashes, or inconsistent member and target metadata.
pub fn read_artifact(path: &Path) -> Result<PackArtifact> {
    let artifact = fs::read(path).map_err(|error| {
        TuffError::new(format!(
            "could not read pack artifact {}: {error}",
            path.display()
        ))
    })?;
    read_artifact_bytes(&artifact)
}

/// Parses and verifies a complete pack artifact already loaded in memory.
///
/// # Errors
///
/// Returns an error for unsupported versions, non-canonical metadata, unsafe paths, truncated or
/// trailing data, invalid hashes, or inconsistent member and target metadata.
pub fn read_artifact_bytes(artifact: &[u8]) -> Result<PackArtifact> {
    let header_len = ARTIFACT_MAGIC.len() + 8;
    if artifact.len() < header_len || &artifact[..ARTIFACT_MAGIC.len()] != ARTIFACT_MAGIC {
        return Err(TuffError::new("invalid pack artifact header"));
    }
    let length_bytes: [u8; 8] = artifact[ARTIFACT_MAGIC.len()..header_len]
        .try_into()
        .map_err(|_| TuffError::new("invalid pack artifact metadata length"))?;
    let metadata_len = usize::try_from(u64::from_be_bytes(length_bytes))
        .map_err(|_| TuffError::new("pack artifact metadata length is too large"))?;
    if metadata_len > MAX_METADATA_BYTES || header_len + metadata_len > artifact.len() {
        return Err(TuffError::new("invalid pack artifact metadata length"));
    }
    let metadata: PackArtifactMetadata =
        serde_json::from_slice(&artifact[header_len..header_len + metadata_len])?;
    validate_artifact_metadata(&metadata)?;
    if serde_json::to_vec(&metadata)? != artifact[header_len..header_len + metadata_len] {
        return Err(TuffError::new(
            "pack artifact metadata is not canonically encoded",
        ));
    }

    let mut cursor = header_len + metadata_len;
    let mut contents = Vec::with_capacity(metadata.files.len());
    for file in &metadata.files {
        let size = usize::try_from(file.size)
            .map_err(|_| TuffError::new("pack artifact file is too large"))?;
        let end = cursor
            .checked_add(size)
            .filter(|end| *end <= artifact.len())
            .ok_or_else(|| {
                TuffError::new(format!("truncated pack artifact file: {}", file.path))
            })?;
        let bytes = artifact[cursor..end].to_vec();
        if sha256(&bytes) != file.sha256 {
            return Err(TuffError::new(format!(
                "pack artifact hash mismatch for {}",
                file.path
            )));
        }
        contents.push(PackArtifactContent {
            path: file.path.clone(),
            bytes,
        });
        cursor = end;
    }
    if cursor != artifact.len() {
        return Err(TuffError::new("pack artifact contains trailing data"));
    }

    Ok(PackArtifact {
        metadata,
        contents,
        digest: sha256(artifact),
    })
}

/// Extracts files below an artifact prefix into a missing or empty directory.
///
/// Returns the number of extracted files.
///
/// # Errors
///
/// Returns an error when the prefix is absent, a path is unsafe, the destination is non-empty, or
/// a filesystem operation fails.
pub fn extract_prefix(artifact: &PackArtifact, prefix: &str, output: &Path) -> Result<usize> {
    let prefix = format!("{}/", prefix.trim_end_matches('/'));
    let selected = artifact
        .contents
        .iter()
        .filter_map(|content| {
            content
                .path
                .strip_prefix(&prefix)
                .map(|relative| (relative, &content.bytes))
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(TuffError::new(format!(
            "pack artifact contains no files under {prefix}"
        )));
    }
    if output.exists() && (!output.is_dir() || fs::read_dir(output)?.next().is_some()) {
        return Err(TuffError::new(format!(
            "pack extraction output must be missing or empty: {}",
            output.display()
        )));
    }
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix("tuff-extract-")
        .tempdir_in(parent)?;
    for (relative, bytes) in &selected {
        let relative = validate_relative_path(Path::new(relative))?;
        let destination = staging.path().join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(destination, bytes)?;
    }
    if output.exists() {
        fs::remove_dir(output)?;
    }
    fs::rename(staging.keep(), output)?;
    Ok(selected.len())
}

/// Normalizes a non-empty relative path and rejects traversal or platform prefixes.
///
/// # Errors
///
/// Returns an error for empty, absolute, parent, current-directory, or prefixed paths.
pub fn validate_relative_path(path: &Path) -> Result<PathBuf> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(TuffError::new(format!(
            "path must be a non-empty relative path: {}",
            path.display()
        )));
    }
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => clean.push(value),
            _ => {
                return Err(TuffError::new(format!(
                    "path traversal is not allowed: {}",
                    path.display()
                )));
            }
        }
    }
    Ok(clean)
}

fn validate_pack_manifest(manifest: &PackManifest) -> Result<()> {
    if manifest.schema != PACK_SCHEMA_VERSION {
        return Err(TuffError::new(format!(
            "unsupported pack schema version: {}",
            manifest.schema
        )));
    }
    validate_non_empty("name", &manifest.name)?;
    validate_non_empty("version", &manifest.version)?;
    validate_non_empty("description", &manifest.description)?;
    if manifest.name.chars().any(char::is_whitespace) {
        return Err(TuffError::new("pack name must not contain whitespace"));
    }
    if manifest.capabilities.is_empty() {
        return Err(TuffError::new("pack must contain at least one capability"));
    }
    if manifest.build.targets.is_empty() {
        return Err(TuffError::new(
            "pack build must contain at least one target",
        ));
    }
    let mut targets = BTreeSet::new();
    for target in &manifest.build.targets {
        validate_non_empty("build.targets", target)?;
        if !targets.insert(target) {
            return Err(TuffError::new(format!(
                "duplicate pack build target: {target}"
            )));
        }
    }
    let mut paths = BTreeSet::new();
    for member in &manifest.capabilities {
        let clean = validate_relative_path(Path::new(&member.path))?;
        if !paths.insert(clean) {
            return Err(TuffError::new(format!(
                "duplicate pack capability path: {}",
                member.path
            )));
        }
    }
    Ok(())
}

fn validate_workflow_closure(members: &[LoadedPackMember]) -> Result<()> {
    let types = members
        .iter()
        .map(|member| (member.manifest.id.as_str(), member.manifest.capability_type))
        .collect::<BTreeMap<_, _>>();
    let workflows = members
        .iter()
        .filter(|member| member.manifest.capability_type == CapabilityType::Workflow)
        .map(|member| (member.manifest.id.as_str(), &member.manifest))
        .collect::<BTreeMap<_, _>>();

    for member in members {
        let Some(workflow) = member.manifest.workflow.as_ref() else {
            continue;
        };
        for requirement in &workflow.requires {
            let actual = types.get(requirement.id.as_str()).ok_or_else(|| {
                TuffError::new(format!(
                    "workflow '{}' requires missing capability '{}' ({})",
                    member.manifest.id, requirement.id, requirement.capability_type
                ))
            })?;
            if *actual != requirement.capability_type {
                return Err(TuffError::new(format!(
                    "workflow '{}' requires '{}' as {}, but the pack member is {}",
                    member.manifest.id, requirement.id, requirement.capability_type, actual
                )));
            }
        }
    }

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for id in workflows.keys() {
        visit_workflow(id, &workflows, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn visit_workflow<'a>(
    id: &'a str,
    workflows: &BTreeMap<&'a str, &'a CapabilityManifest>,
    visiting: &mut BTreeSet<&'a str>,
    visited: &mut BTreeSet<&'a str>,
) -> Result<()> {
    if visited.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id) {
        return Err(TuffError::new(format!(
            "workflow dependency cycle contains '{id}'"
        )));
    }
    if let Some(workflow) = workflows.get(id).and_then(|item| item.workflow.as_ref()) {
        for requirement in &workflow.requires {
            if requirement.capability_type == CapabilityType::Workflow
                && workflows.contains_key(requirement.id.as_str())
            {
                visit_workflow(requirement.id.as_str(), workflows, visiting, visited)?;
            }
        }
    }
    visiting.remove(id);
    visited.insert(id);
    Ok(())
}

fn validate_artifact_metadata(metadata: &PackArtifactMetadata) -> Result<()> {
    if metadata.artifact_version != PACK_ARTIFACT_VERSION {
        return Err(TuffError::new(format!(
            "unsupported pack artifact version: {}",
            metadata.artifact_version
        )));
    }
    if metadata.pack_schema != PACK_SCHEMA_VERSION {
        return Err(TuffError::new(format!(
            "unsupported pack schema version: {}",
            metadata.pack_schema
        )));
    }
    validate_non_empty("name", &metadata.name)?;
    validate_non_empty("version", &metadata.version)?;
    if metadata.capabilities.is_empty() {
        return Err(TuffError::new(
            "pack artifact must contain at least one capability",
        ));
    }
    if metadata.targets.is_empty() {
        return Err(TuffError::new(
            "pack artifact must contain at least one target",
        ));
    }
    if !metadata
        .capabilities
        .windows(2)
        .all(|window| window[0].id < window[1].id)
    {
        return Err(TuffError::new(
            "pack artifact capabilities are not canonically ordered",
        ));
    }
    if !metadata
        .targets
        .windows(2)
        .all(|window| window[0].id < window[1].id)
    {
        return Err(TuffError::new(
            "pack artifact targets are not canonically ordered",
        ));
    }
    for capability in &metadata.capabilities {
        validate_non_empty("capability.id", &capability.id)?;
        validate_non_empty("capability.version", &capability.version)?;
        validate_relative_path(Path::new(&capability.source_path))?;
        if capability.capability_type == CapabilityType::Policy {
            return Err(TuffError::new(
                "policy capabilities are not supported in pack artifacts",
            ));
        }
    }
    let capability_ids = metadata
        .capabilities
        .iter()
        .map(|capability| capability.id.as_str())
        .collect::<Vec<_>>();
    let artifact_paths = metadata
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<BTreeSet<_>>();
    for capability in &metadata.capabilities {
        let manifest_path = format!("sources/{}/tuff.toml", capability.id);
        if !artifact_paths.contains(manifest_path.as_str()) {
            return Err(TuffError::new(format!(
                "pack artifact is missing source manifest for '{}'",
                capability.id
            )));
        }
    }
    for target in &metadata.targets {
        validate_non_empty("target.id", &target.id)?;
        if !target
            .capabilities
            .windows(2)
            .all(|window| window[0].id < window[1].id)
        {
            return Err(TuffError::new(format!(
                "pack artifact target '{}' capabilities are not canonically ordered",
                target.id
            )));
        }
        let target_ids = target
            .capabilities
            .iter()
            .map(|capability| capability.id.as_str())
            .collect::<Vec<_>>();
        if target_ids != capability_ids {
            return Err(TuffError::new(format!(
                "pack artifact target '{}' does not describe every capability",
                target.id
            )));
        }
        for capability in &target.capabilities {
            validate_relative_path(Path::new(&capability.installed_path))?;
            crate::cache::validate_hash(&capability.sha256)?;
            for path in &capability.emitted_files {
                validate_relative_path(Path::new(path))?;
                let artifact_path = format!("targets/{}/{path}", target.id);
                if !artifact_paths.contains(artifact_path.as_str()) {
                    return Err(TuffError::new(format!(
                        "pack artifact is missing emitted file '{}' for target '{}'",
                        path, target.id
                    )));
                }
            }
            for hook in &capability.managed_hooks {
                validate_relative_path(Path::new(&hook.settings_path))?;
                crate::cache::validate_hash(&hook.baseline_hash)?;
                let artifact_path = format!("targets/{}/{}", target.id, hook.settings_path);
                if !artifact_paths.contains(artifact_path.as_str()) {
                    return Err(TuffError::new(format!(
                        "pack artifact is missing hook settings '{}' for target '{}'",
                        hook.settings_path, target.id
                    )));
                }
            }
        }
    }
    let mut paths = BTreeSet::new();
    for file in &metadata.files {
        validate_relative_path(Path::new(&file.path))?;
        if !file.path.starts_with("sources/") && !file.path.starts_with("targets/") {
            return Err(TuffError::new(format!(
                "unsupported pack artifact file path: {}",
                file.path
            )));
        }
        crate::cache::validate_hash(&file.sha256)?;
        if !paths.insert(file.path.as_str()) {
            return Err(TuffError::new(format!(
                "duplicate pack artifact path: {}",
                file.path
            )));
        }
    }
    if !metadata
        .files
        .windows(2)
        .all(|window| window[0].path < window[1].path)
    {
        return Err(TuffError::new(
            "pack artifact file metadata is not canonically ordered",
        ));
    }
    Ok(())
}

fn insert_content(
    contents: &mut BTreeMap<String, Vec<u8>>,
    path: String,
    bytes: Vec<u8>,
) -> Result<()> {
    if contents.insert(path.clone(), bytes).is_some() {
        return Err(TuffError::new(format!(
            "duplicate pack artifact path: {path}"
        )));
    }
    Ok(())
}

fn read_safe_member_file(pack_root: &Path, path: &Path) -> Result<Vec<u8>> {
    let relative = path
        .strip_prefix(pack_root)
        .map_err(|_| TuffError::new("pack member source escaped the pack root"))?;
    reject_symlink_path(pack_root, relative)?;
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(TuffError::new(format!(
            "pack member source must be a regular file: {}",
            path.display()
        )));
    }
    let canonical = path.canonicalize()?;
    if !canonical.starts_with(pack_root) {
        return Err(TuffError::new(format!(
            "pack member source escaped the pack root: {}",
            path.display()
        )));
    }
    Ok(fs::read(canonical)?)
}

fn reject_symlink_path(root: &Path, relative: &Path) -> Result<()> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err(TuffError::new("invalid pack member path"));
        };
        current.push(value);
        if fs::symlink_metadata(&current)?.file_type().is_symlink() {
            return Err(TuffError::new(format!(
                "symbolic links are not allowed in pack member paths: {}",
                current.display()
            )));
        }
    }
    Ok(())
}

fn validate_non_empty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(TuffError::new(format!(
            "pack manifest field '{field}' must be a non-empty string"
        )));
    }
    Ok(())
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> PackArtifactMetadata {
        PackArtifactMetadata {
            artifact_version: PACK_ARTIFACT_VERSION,
            pack_schema: PACK_SCHEMA_VERSION,
            name: "com.acme/demo".into(),
            version: "1.0.0".into(),
            description: "Demo pack".into(),
            capabilities: vec![PackArtifactCapability {
                id: "demo".into(),
                capability_type: CapabilityType::Skill,
                version: "1.0.0".into(),
                description: "Demo capability".into(),
                source_path: "capabilities/demo".into(),
            }],
            targets: vec![PackArtifactTarget {
                id: "demo".into(),
                capabilities: vec![PackArtifactTargetCapability {
                    id: "demo".into(),
                    installed_path: ".agents/skills/demo".into(),
                    sha256: sha256(b"tree"),
                    emitted_files: Vec::new(),
                    managed_hooks: Vec::new(),
                }],
            }],
            files: Vec::new(),
        }
    }

    #[test]
    fn artifact_build_is_byte_for_byte_deterministic() {
        let temp = tempfile::tempdir().unwrap();
        let left = temp.path().join("left.tuffpack");
        let right = temp.path().join("right.tuffpack");
        let contents = || {
            vec![
                PackArtifactContent {
                    path: "targets/demo/b".into(),
                    bytes: b"b".to_vec(),
                },
                PackArtifactContent {
                    path: "targets/demo/a".into(),
                    bytes: b"a".to_vec(),
                },
            ]
        };

        write_artifact(&left, metadata(), contents()).unwrap();
        write_artifact(&right, metadata(), contents()).unwrap();

        assert_eq!(fs::read(left).unwrap(), fs::read(right).unwrap());
    }

    #[test]
    fn artifact_read_rejects_tampered_payload() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("demo.tuffpack");
        write_artifact(
            &path,
            metadata(),
            vec![
                PackArtifactContent {
                    path: "sources/demo/tuff.toml".into(),
                    bytes: b"manifest".to_vec(),
                },
                PackArtifactContent {
                    path: "targets/demo/file".into(),
                    bytes: b"safe".to_vec(),
                },
            ],
        )
        .unwrap();
        let mut bytes = fs::read(&path).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(&path, bytes).unwrap();

        let error = read_artifact(&path).unwrap_err();

        assert!(error.to_string().contains("hash mismatch"));
    }

    #[test]
    fn relative_path_rejects_parent_traversal() {
        let error = validate_relative_path(Path::new("../secret")).unwrap_err();

        assert!(error.to_string().contains("path traversal"));
    }
}
