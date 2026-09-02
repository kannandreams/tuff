use std::{
    collections::BTreeMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Result, TuffError};
use crate::manifest::{CapabilityType, ImplementationConfig, McpServerConfig, WorkflowConfig};

/// Current on-disk schema. Older readable versions are migrated in memory
/// by `read_lockfile_at`; writers always emit this version.
pub const LOCKFILE_VERSION: u8 = 2;
/// Oldest schema this build still reads.
pub const OLDEST_READABLE_LOCKFILE_VERSION: u8 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct Lockfile {
    /// The schema version the file was read as, or `LOCKFILE_VERSION` for a
    /// lockfile built in memory. Writers ignore it and emit the current one.
    pub version: u8,
    pub capabilities: BTreeMap<String, CapabilityLockEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityLockEntry {
    #[serde(rename = "type")]
    pub capability_type: CapabilityType,
    /// The capability's own version: a declared manifest version, or the
    /// commit that was installed when nothing better exists. Which one is
    /// recorded in `version_scheme`, never guessed from the string.
    pub version: String,
    #[serde(default)]
    pub version_scheme: VersionScheme,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Where this capability came from. One typed value, so every lifecycle
    /// verb dispatches on `match` instead of comparing strings.
    pub source: CapabilitySource,
    pub targets: BTreeMap<String, TargetLockEntry>,
    /// Cached from the manifest at install/update time, the same way
    /// `description` is: after install, only the `files` a manifest declares
    /// get copied to disk, `tuff.toml` itself does not, so this is the only
    /// durable record of how a tool is invoked. Consumed by the generated
    /// capability-index skill (RFC-103 tier 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation: Option<ImplementationConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
    /// Same rationale as `implementation`/`parameters`: a workflow's
    /// `requires` list lives only in its manifest, which isn't copied to the
    /// installed target directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<WorkflowConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<McpServerConfig>,
}

/// What kind of string `CapabilityLockEntry::version` holds (RFC-105 D4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionScheme {
    /// A release chosen by semver tag resolution (RFC-101; not written yet).
    Semver,
    /// The version the manifest declares. Says nothing about releases.
    #[default]
    Declared,
    /// A commit SHA: content-exact, semantically silent.
    Sha,
}

/// The origin of an installed capability. Internally tagged as `kind` on
/// the wire, so a lockfile row reads `[capabilities.source] kind = "git"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CapabilitySource {
    Local(LocalSource),
    Git(GitSource),
    Catalog(CatalogSource),
    Pack(PackProvenance),
}

impl CapabilitySource {
    pub fn local(path: impl Into<String>) -> Self {
        Self::Local(LocalSource { path: path.into() })
    }

    /// The `kind` string as written to the lockfile.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Local(_) => "local",
            Self::Git(_) => "git",
            Self::Catalog(_) => "catalog",
            Self::Pack(_) => "pack",
        }
    }

    pub fn as_git(&self) -> Option<&GitSource> {
        match self {
            Self::Git(git) => Some(git),
            _ => None,
        }
    }

    pub fn as_pack(&self) -> Option<&PackProvenance> {
        match self {
            Self::Pack(pack) => Some(pack),
            _ => None,
        }
    }

    /// The local path a capability was installed from, when it has one.
    pub fn local_path(&self) -> Option<&str> {
        match self {
            Self::Local(local) => Some(local.path.as_str()),
            _ => None,
        }
    }

    /// The version scheme a fresh install from this source records. Git
    /// installs pin a commit until RFC-101 resolves tags; everything else
    /// carries the version its manifest declared.
    pub fn default_version_scheme(&self) -> VersionScheme {
        match self {
            Self::Git(_) => VersionScheme::Sha,
            _ => VersionScheme::Declared,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalSource {
    /// Path to the source directory, relative to the lockfile's root when it
    /// lies inside it, absolute otherwise. Empty for an adopted capability
    /// whose only copy is the installed tree.
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitSource {
    pub url: String,
    /// Subdirectory within the repository holding the capability.
    #[serde(default)]
    pub path: String,
    /// The commit that was installed. Always present.
    #[serde(rename = "ref")]
    pub git_ref: String,
    /// The tag that chose `ref`, when one did (RFC-101).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// The range the user asked for, when they did (RFC-101).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogSource {
    /// The built-in catalog entry id.
    pub id: String,
    /// That entry's version at install time.
    pub version: String,
}

/// Immutable pack release that delivered a capability entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackProvenance {
    pub name: String,
    pub version: String,
    /// Artifact digest, bare lowercase hex. `sha256:` prefixes exist only at
    /// the OCI boundary.
    pub digest: String,
    /// The OCI registry and repository this pack was pulled from
    /// ("registry/repository", no tag), when known.
    ///
    /// `tuff add pack` only ever sees a local artifact file; it has no way to
    /// know where that file came from unless the caller says so with
    /// `--reference`. Absent, `tuff outdated` cannot check this capability
    /// against anything and reports it as such rather than guessing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    /// The member's path inside the pack's `sources/` tree.
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetLockEntry {
    #[serde(
        default,
        rename = "managedHooks",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub managed_hooks: Vec<ManagedHook>,
    #[serde(
        default,
        rename = "managedMcpEntry",
        skip_serializing_if = "Option::is_none"
    )]
    pub managed_mcp_entry: Option<ManagedMcpEntry>,
    #[serde(default)]
    pub ownership: TargetOwnership,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub installed_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedHook {
    #[serde(rename = "settingsPath")]
    pub settings_path: String,
    pub event: String,
    #[serde(
        default,
        rename = "canonicalEvent",
        skip_serializing_if = "Option::is_none"
    )]
    pub canonical_event: Option<String>,
    pub command: String,
    #[serde(rename = "baselineHash")]
    pub baseline_hash: String,
}

/// Baseline for one Tuff-managed `mcpServers.<id>` entry (RFC-102 stage b).
///
/// MCP config files are shared ground that users hand-edit, so the entry
/// gets the managed-hook treatment: a content hash recorded at registration
/// time, compared on every `check`/`list`, never whole-file ownership. The
/// entry's key is the capability id, so only the file path and hash are
/// stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedMcpEntry {
    #[serde(rename = "configPath")]
    pub config_path: String,
    #[serde(rename = "baselineHash")]
    pub baseline_hash: String,
}

/// Hash an MCP entry value exactly as `managed_mcp_entry_status` will when
/// it re-reads the file: canonical `serde_json` bytes, so on-disk pretty-
/// printing never matters.
pub fn managed_mcp_entry_baseline(entry: &serde_json::Value) -> Result<String> {
    Ok(hash_bytes(&serde_json::to_vec(entry)?))
}

/// `"clean"`, `"modified"`, or `"missing"` for a managed MCP entry.
pub fn managed_mcp_entry_status(
    repo_root: &Path,
    capability_id: &str,
    entry: &ManagedMcpEntry,
) -> &'static str {
    let path = repo_root.join(&entry.config_path);
    let Ok(raw) = std::fs::read_to_string(path) else {
        return "missing";
    };
    let Ok(config): std::result::Result<serde_json::Value, _> = serde_json::from_str(&raw) else {
        return "modified";
    };
    let Some(current) = config
        .get("mcpServers")
        .and_then(|servers| servers.get(capability_id))
    else {
        return "missing";
    };
    match serde_json::to_vec(current) {
        Ok(bytes) if hash_bytes(&bytes) == entry.baseline_hash => "clean",
        _ => "modified",
    }
}

pub fn managed_hooks_from_fragment(
    repo_root: &Path,
    settings_path: &str,
    fragment: &serde_json::Value,
) -> Result<Vec<ManagedHook>> {
    managed_hooks_from_fragment_with_canonical(repo_root, settings_path, fragment, None)
}

pub fn managed_hooks_from_fragment_with_canonical(
    _repo_root: &Path,
    settings_path: &str,
    fragment: &serde_json::Value,
    canonical_event: Option<&str>,
) -> Result<Vec<ManagedHook>> {
    let mut managed = Vec::new();
    let Some(events) = fragment.get("hooks").and_then(serde_json::Value::as_object) else {
        return Ok(managed);
    };

    for (event, groups) in events {
        let Some(groups) = groups.as_array() else {
            continue;
        };
        for group in groups {
            let hooks = group
                .get("hooks")
                .and_then(serde_json::Value::as_array)
                .map_or_else(|| vec![group], |hooks| hooks.iter().collect());
            for hook in hooks {
                let Some(command) = hook.get("command").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                let baseline = serde_json::to_vec(hook)?;
                managed.push(ManagedHook {
                    settings_path: settings_path.to_string(),
                    event: event.clone(),
                    canonical_event: canonical_event.map(str::to_owned),
                    command: command.to_string(),
                    baseline_hash: hash_bytes(&baseline),
                });
            }
        }
    }
    Ok(managed)
}

pub fn managed_hook_status(repo_root: &Path, hook: &ManagedHook) -> &'static str {
    let path = repo_root.join(&hook.settings_path);
    let Ok(settings) = std::fs::read_to_string(path) else {
        return "missing";
    };
    let Ok(settings): std::result::Result<serde_json::Value, _> = serde_json::from_str(&settings)
    else {
        return "modified";
    };
    let Some(groups) = settings
        .get("hooks")
        .and_then(|hooks| hooks.get(&hook.event))
        .and_then(serde_json::Value::as_array)
    else {
        return "missing";
    };

    for group in groups {
        let entries = group
            .get("hooks")
            .and_then(serde_json::Value::as_array)
            .map_or_else(|| vec![group], |entries| entries.iter().collect());
        for entry in entries {
            if entry.get("command").and_then(serde_json::Value::as_str)
                == Some(hook.command.as_str())
            {
                let Ok(content) = serde_json::to_vec(entry) else {
                    return "modified";
                };
                return if hash_bytes(&content) == hook.baseline_hash {
                    "clean"
                } else {
                    "modified"
                };
            }
        }
    }
    "missing"
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetOwnership {
    #[default]
    Generated,
    Imported,
}

/// The project-scope lockfile. Never falls through to the global one: the
/// caller resolved a scope and this is the file for it (RFC-105 D3).
pub fn project_lockfile(repo_root: &Path) -> PathBuf {
    repo_root.join("tuff.lock")
}

/// The lockfile for a resolved scope: `<root>/tuff.lock` for a project,
/// the XDG state file for the global scope (where `scope_root` is the home
/// directory). The scope is always passed, never inferred from the path.
pub fn scoped_lockfile(scope_root: &Path, scope: crate::resolver::Scope) -> PathBuf {
    match scope {
        crate::resolver::Scope::Project => project_lockfile(scope_root),
        crate::resolver::Scope::Global => crate::paths::global_lockfile(scope_root),
    }
}

pub fn require_scoped_lockfile(
    scope_root: &Path,
    scope: crate::resolver::Scope,
) -> Result<Lockfile> {
    read_lockfile_at(&scoped_lockfile(scope_root, scope))
}

pub fn write_scoped_lockfile(
    scope_root: &Path,
    scope: crate::resolver::Scope,
    lockfile: &Lockfile,
) -> Result<()> {
    write_lockfile_at(&scoped_lockfile(scope_root, scope), lockfile)
}

pub fn init_lockfile(repo_root: &Path) -> Result<PathBuf> {
    let lock_path = project_lockfile(repo_root);
    init_lockfile_at(&lock_path)?;
    Ok(lock_path)
}

pub fn init_lockfile_at(lock_path: &Path) -> Result<()> {
    if !lock_path.exists() {
        write_lockfile_at(
            lock_path,
            &Lockfile {
                version: LOCKFILE_VERSION,
                capabilities: BTreeMap::new(),
            },
        )?;
    }
    Ok(())
}

pub fn require_lockfile(repo_root: &Path) -> Result<Lockfile> {
    read_lockfile_at(&project_lockfile(repo_root))
}

/// Read a lockfile of any supported schema version into the current model.
///
/// The version is read before anything else is deserialised, so a file from
/// a newer tuff fails with a message about versions rather than a shape
/// error naming some field the reader has never heard of.
pub fn read_lockfile_at(path: &Path) -> Result<Lockfile> {
    if !path.exists() {
        let parent = path.parent().unwrap_or(Path::new("."));
        return Err(TuffError::new(format!(
            "{} is missing; run 'tuff init' first",
            parent
                .join(path.file_name().unwrap_or(OsStr::new("tuff.lock")))
                .display()
        )));
    }
    let raw = std::fs::read_to_string(path)?;
    let version = peek_version(&raw, path)?;
    let rows: Vec<Row> = match version {
        1 => read_v1_rows(&raw)?,
        2 => read_v2_rows(&raw)?,
        newer => {
            return Err(TuffError::new(format!(
                "unsupported lockfile version: {newer} ({} was written by a newer tuff; this tuff {} reads versions {OLDEST_READABLE_LOCKFILE_VERSION} to {LOCKFILE_VERSION}, upgrade tuff)",
                path.display(),
                env!("CARGO_PKG_VERSION")
            )));
        }
    };
    let mut capabilities: BTreeMap<String, CapabilityLockEntry> = BTreeMap::new();
    for row in rows {
        let Row {
            name,
            target,
            target_entry,
            entry,
        } = row;
        match capabilities.entry(name) {
            std::collections::btree_map::Entry::Occupied(mut existing) => {
                existing.get_mut().targets.insert(target, target_entry);
            }
            std::collections::btree_map::Entry::Vacant(slot) => {
                let mut entry = entry;
                entry.targets.insert(target, target_entry);
                slot.insert(entry);
            }
        }
    }
    Ok(Lockfile {
        version,
        capabilities,
    })
}

/// One wire row folded to its capability entry plus its target.
struct Row {
    name: String,
    target: String,
    target_entry: TargetLockEntry,
    entry: CapabilityLockEntry,
}

fn peek_version(raw: &str, path: &Path) -> Result<u8> {
    #[derive(Deserialize)]
    struct VersionOnly {
        version: Option<u8>,
    }
    let peek: VersionOnly = toml::from_str(raw).map_err(|error| {
        TuffError::new(format!(
            "{} is not a valid lockfile: {}",
            path.display(),
            error.message()
        ))
    })?;
    match peek.version {
        Some(version) if version >= OLDEST_READABLE_LOCKFILE_VERSION => Ok(version),
        Some(version) => Err(TuffError::new(format!(
            "unsupported lockfile version: {version} ({} predates every schema this tuff reads)",
            path.display()
        ))),
        None => Err(TuffError::new(format!(
            "{} has no version field; it is not a Tuff lockfile or it is corrupt",
            path.display()
        ))),
    }
}

/// Schema version 1, read for migration only (RFC-105 D5). Never written.
fn read_v1_rows(raw: &str) -> Result<Vec<Row>> {
    let wire: WireLockfileV1 = toml::from_str(raw)
        .map_err(|error| TuffError::new(format!("invalid version 1 lockfile: {error}")))?;
    Ok(wire
        .capabilities
        .into_iter()
        .map(|item| {
            let source = match item.pack {
                // A pack member was written as "local" with an empty path
                // plus a pack table; the pack is the real origin. The member
                // path inside the pack was not recorded in v1, and the member
                // id is what `tuff add pack` used, so it is the best backfill.
                Some(pack) => CapabilitySource::Pack(PackProvenance {
                    name: pack.name,
                    version: pack.version,
                    digest: pack.digest,
                    registry: pack.registry,
                    path: item.name.clone(),
                }),
                None => match item.source.as_str() {
                    "git" => CapabilitySource::Git(GitSource {
                        url: item.repository,
                        path: item.source_path,
                        git_ref: item.resolved_ref,
                        tag: None,
                        requested: None,
                    }),
                    "catalog" => CapabilitySource::Catalog(CatalogSource {
                        id: item.source_path,
                        version: item.resolved_ref,
                    }),
                    // The generated capability index wrote a sentinel path
                    // in v1; it has no source tree and v2 says so plainly.
                    _ if item.source_path == "<generated>" => CapabilitySource::local(""),
                    _ => CapabilitySource::local(item.source_path),
                },
            };
            let version_scheme = source.default_version_scheme();
            Row {
                name: item.name,
                target: item.target,
                target_entry: TargetLockEntry {
                    managed_hooks: item.managed_hooks,
                    managed_mcp_entry: item.managed_mcp_entry,
                    ownership: item.ownership,
                    sha256: item.sha256,
                    installed_path: item.installed_path,
                },
                entry: CapabilityLockEntry {
                    capability_type: item.capability_type,
                    version: item.version,
                    version_scheme,
                    description: item.description,
                    source,
                    targets: BTreeMap::new(),
                    implementation: item.implementation,
                    parameters: item.parameters,
                    workflow: item.workflow,
                    server: item.server,
                },
            }
        })
        .collect())
}

fn read_v2_rows(raw: &str) -> Result<Vec<Row>> {
    let wire: WireLockfile = toml::from_str(raw)
        .map_err(|error| TuffError::new(format!("invalid lockfile: {error}")))?;
    Ok(wire
        .capabilities
        .into_iter()
        .map(|item| Row {
            name: item.name,
            target: item.target,
            target_entry: TargetLockEntry {
                managed_hooks: item.managed_hooks,
                managed_mcp_entry: item.managed_mcp_entry,
                ownership: item.ownership,
                sha256: item.sha256,
                installed_path: item.installed_path,
            },
            entry: CapabilityLockEntry {
                capability_type: item.capability_type,
                version: item.version,
                version_scheme: item.version_scheme,
                description: item.description,
                source: item.source,
                targets: BTreeMap::new(),
                implementation: item.implementation,
                parameters: item.parameters,
                workflow: item.workflow,
                server: item.server,
            },
        })
        .collect())
}

pub fn write_lockfile(repo_root: &Path, lockfile: &Lockfile) -> Result<()> {
    write_lockfile_at(&project_lockfile(repo_root), lockfile)
}

pub fn write_lockfile_at(path: &Path, lockfile: &Lockfile) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut capabilities = Vec::new();
    for (name, entry) in &lockfile.capabilities {
        for (target, target_entry) in &entry.targets {
            capabilities.push(WireCapability {
                name: name.clone(),
                capability_type: entry.capability_type,
                version: entry.version.clone(),
                version_scheme: entry.version_scheme,
                description: entry.description.clone(),
                target: target.clone(),
                installed_path: target_entry.installed_path.clone(),
                sha256: target_entry.sha256.clone(),
                ownership: target_entry.ownership,
                source: entry.source.clone(),
                managed_hooks: target_entry.managed_hooks.clone(),
                managed_mcp_entry: target_entry.managed_mcp_entry.clone(),
                implementation: entry.implementation.clone(),
                parameters: entry.parameters.clone(),
                workflow: entry.workflow.clone(),
                server: entry.server.clone(),
            });
        }
    }
    capabilities.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.capability_type.as_str().cmp(b.capability_type.as_str()))
            .then_with(|| a.target.cmp(&b.target))
            .then_with(|| a.installed_path.cmp(&b.installed_path))
    });
    let wire = WireLockfile {
        version: LOCKFILE_VERSION,
        capabilities,
    };
    let content = format!(
        "# Tuff lockfile. Each entry records one capability installation target.\n{}\n",
        toml::to_string_pretty(&wire)?
    );
    std::fs::write(path, content)?;
    Ok(())
}

/// Schema version 2 (RFC-105 D1). Scalars first, tables after, so the TOML
/// serializer never has to emit a value beneath a table.
#[derive(Debug, Serialize, Deserialize)]
struct WireLockfile {
    version: u8,
    capabilities: Vec<WireCapability>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WireCapability {
    name: String,
    #[serde(rename = "type")]
    capability_type: CapabilityType,
    #[serde(default)]
    version: String,
    #[serde(default)]
    version_scheme: VersionScheme,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
    target: String,
    installed_path: String,
    sha256: String,
    #[serde(default)]
    ownership: TargetOwnership,
    source: CapabilitySource,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    managed_hooks: Vec<ManagedHook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    managed_mcp_entry: Option<ManagedMcpEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    implementation: Option<ImplementationConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parameters: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workflow: Option<WorkflowConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    server: Option<McpServerConfig>,
}

/// Schema version 1 as tuff 0.1.x wrote it. Read-only; see `read_v1_rows`.
#[derive(Debug, Deserialize)]
struct WireLockfileV1 {
    #[allow(dead_code)]
    version: u8,
    capabilities: Vec<WireCapabilityV1>,
}

#[derive(Debug, Deserialize)]
struct WireCapabilityV1 {
    name: String,
    #[serde(rename = "type")]
    capability_type: CapabilityType,
    source: String,
    #[serde(default)]
    repository: String,
    #[serde(default)]
    source_path: String,
    #[serde(default)]
    resolved_ref: String,
    sha256: String,
    target: String,
    installed_path: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    ownership: TargetOwnership,
    #[serde(default)]
    managed_hooks: Vec<ManagedHook>,
    #[serde(default)]
    managed_mcp_entry: Option<ManagedMcpEntry>,
    #[serde(default)]
    pack: Option<PackProvenanceV1>,
    #[serde(default)]
    implementation: Option<ImplementationConfig>,
    #[serde(default)]
    parameters: Option<serde_json::Value>,
    #[serde(default)]
    workflow: Option<WorkflowConfig>,
    #[serde(default)]
    server: Option<McpServerConfig>,
}

#[derive(Debug, Deserialize)]
struct PackProvenanceV1 {
    name: String,
    version: String,
    digest: String,
    #[serde(default)]
    registry: Option<String>,
}

pub fn hash_bytes(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}

pub fn relative_or_absolute_fs(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().to_string())
}

pub fn absolutize(repo_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn init_lockfile_at_creates_new_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tuff.lock");
        init_lockfile_at(&path).unwrap();
        assert!(path.exists());

        let lf = read_lockfile_at(&path).unwrap();
        assert_eq!(lf.version, LOCKFILE_VERSION);
        assert!(lf.capabilities.is_empty());
    }

    #[test]
    fn read_lockfile_at_rejects_missing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tuff.lock");
        assert!(read_lockfile_at(&path).is_err());
    }

    #[test]
    fn read_lockfile_at_rejects_v4_schema() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tuff.lock");
        fs::write(&path, "version = 4\ncapabilities = []\n").unwrap();

        let error = read_lockfile_at(&path).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unsupported lockfile version: 4")
        );
    }

    #[test]
    fn write_and_read_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tuff.lock");
        let mut lf = Lockfile {
            version: LOCKFILE_VERSION,
            capabilities: BTreeMap::new(),
        };
        lf.capabilities.insert(
            "test".into(),
            CapabilityLockEntry {
                capability_type: CapabilityType::Skill,
                version: "1.0".into(),
                version_scheme: VersionScheme::Declared,
                description: "test skill".into(),
                source: CapabilitySource::local(""),
                targets: BTreeMap::from([(
                    "open-agents".into(),
                    TargetLockEntry {
                        managed_hooks: Vec::new(),
                        managed_mcp_entry: None,
                        ownership: TargetOwnership::Generated,
                        sha256: hash_bytes(b"content"),
                        installed_path: ".agents/skills/test".into(),
                    },
                )]),
                implementation: None,
                parameters: None,
                workflow: None,
                server: None,
            },
        );
        write_lockfile_at(&path, &lf).unwrap();
        let read = read_lockfile_at(&path).unwrap();
        assert_eq!(read.capabilities.len(), 1);
    }

    #[test]
    fn missing_target_ownership_defaults_to_generated() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tuff.lock");
        fs::write(&path, "version = 1\ncapabilities = []\n").unwrap();
        let read = read_lockfile_at(&path).unwrap();
        assert!(read.capabilities.is_empty());
    }

    #[test]
    fn hash_bytes_produces_consistent_output() {
        let h1 = hash_bytes(b"hello");
        let h2 = hash_bytes(b"hello");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
        assert_ne!(h1, hash_bytes(b"world"));
    }

    #[test]
    fn a_version_1_lockfile_migrates_every_source_kind() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tuff.lock");
        fs::write(
            &path,
            r#"version = 1

[[capabilities]]
name = "git-skill"
type = "skill"
source = "git"
repository = "https://example.com/skills.git"
source_path = "skills/git-skill"
resolved_ref = "9b9c499"
sha256 = "aa"
target = "open-agents"
installed_path = ".agents/skills/git-skill"
version = "9b9c499"

[[capabilities]]
name = "memory"
type = "mcp-server"
source = "catalog"
repository = "builtin"
source_path = "memory"
resolved_ref = "1.0.0"
sha256 = "bb"
target = "open-agents"
installed_path = ".agents/mcp-servers/memory"
version = "1.0.0"

[[capabilities]]
name = "pack-skill"
type = "skill"
source = "local"
source_path = ""
resolved_ref = ""
sha256 = "cc"
target = "open-agents"
installed_path = ".agents/skills/pack-skill"
version = "1.5.0"

[capabilities.pack]
name = "com.acme/fixture"
version = "1.0.0"
digest = "dd"
registry = "ghcr.io/acme/fixture"

[[capabilities]]
name = "local-skill"
type = "skill"
source = "local"
source_path = "sources/local-skill"
resolved_ref = ""
sha256 = "ee"
target = "open-agents"
installed_path = ".agents/skills/local-skill"
version = "1.0.0"
"#,
        )
        .unwrap();

        let lf = read_lockfile_at(&path).unwrap();
        assert_eq!(lf.version, 1, "the version read is reported, not rewritten");
        assert_eq!(
            lf.capabilities["git-skill"].source,
            CapabilitySource::Git(GitSource {
                url: "https://example.com/skills.git".into(),
                path: "skills/git-skill".into(),
                git_ref: "9b9c499".into(),
                tag: None,
                requested: None,
            })
        );
        assert_eq!(
            lf.capabilities["git-skill"].version_scheme,
            VersionScheme::Sha
        );
        assert_eq!(
            lf.capabilities["memory"].source,
            CapabilitySource::Catalog(CatalogSource {
                id: "memory".into(),
                version: "1.0.0".into(),
            })
        );
        assert_eq!(
            lf.capabilities["pack-skill"].source,
            CapabilitySource::Pack(PackProvenance {
                name: "com.acme/fixture".into(),
                version: "1.0.0".into(),
                digest: "dd".into(),
                registry: Some("ghcr.io/acme/fixture".into()),
                path: "pack-skill".into(),
            })
        );
        assert_eq!(
            lf.capabilities["local-skill"].source,
            CapabilitySource::local("sources/local-skill")
        );
        assert_eq!(
            lf.capabilities["local-skill"].version_scheme,
            VersionScheme::Declared
        );

        // Writing produces v2, and v2 round-trips byte for byte.
        write_lockfile_at(&path, &lf).unwrap();
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("version = 2\n"));
        assert!(written.contains("kind = \"pack\""));
        assert!(!written.contains("resolved_ref"));
        let again = read_lockfile_at(&path).unwrap();
        assert_eq!(again.version, 2);
        write_lockfile_at(&path, &again).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), written);
    }

    #[test]
    fn a_lockfile_without_a_version_is_corrupt_not_empty() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tuff.lock");
        fs::write(&path, "capabilities = []\n").unwrap();
        let error = read_lockfile_at(&path).unwrap_err().to_string();
        assert!(error.contains("no version field"), "{error}");

        fs::write(&path, "version = 2\n[[capabilities]\n").unwrap();
        let error = read_lockfile_at(&path).unwrap_err().to_string();
        assert!(error.contains("not a valid lockfile"), "{error}");
    }

    #[test]
    fn managed_mcp_entry_status_tracks_the_entry_not_the_file() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("mcp.json");
        let entry_value = serde_json::json!({"command": "npx", "args": ["-y", "srv"]});
        let both = |neighbour: &str| {
            serde_json::to_string_pretty(&serde_json::json!({
                "mcpServers": {"github": entry_value, "neighbour": {"command": neighbour}}
            }))
            .unwrap()
        };
        fs::write(&config_path, both("hand")).unwrap();
        let managed = ManagedMcpEntry {
            config_path: "mcp.json".into(),
            baseline_hash: managed_mcp_entry_baseline(&entry_value).unwrap(),
        };

        // Pretty-printing and neighbouring hand-written entries never matter,
        // and editing the neighbour leaves ours clean.
        assert_eq!(
            managed_mcp_entry_status(tmp.path(), "github", &managed),
            "clean"
        );
        fs::write(&config_path, both("edited")).unwrap();
        assert_eq!(
            managed_mcp_entry_status(tmp.path(), "github", &managed),
            "clean"
        );

        // Editing our entry is modified; removing it, or the file, is missing.
        fs::write(
            &config_path,
            r#"{"mcpServers": {"github": {"command": "tampered"}}}"#,
        )
        .unwrap();
        assert_eq!(
            managed_mcp_entry_status(tmp.path(), "github", &managed),
            "modified"
        );
        fs::write(&config_path, r#"{"mcpServers": {}}"#).unwrap();
        assert_eq!(
            managed_mcp_entry_status(tmp.path(), "github", &managed),
            "missing"
        );
        fs::remove_file(&config_path).unwrap();
        assert_eq!(
            managed_mcp_entry_status(tmp.path(), "github", &managed),
            "missing"
        );
    }
}
