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
///
/// Versions 1 and 2 are TOML. Version 3 carries the same rows as version 2
/// encoded as JSON, in the layout `JSON.stringify(value, null, 2)` produces:
/// two-space indent, one array element per line, a trailing newline. That
/// is the layout npm, jq, Python, and VS Code's JSON formatter all agree
/// on, so a formatter that a repository runs over its JSON files leaves
/// the lockfile byte for byte unchanged instead of rewriting it.
pub const LOCKFILE_VERSION: u8 = 3;
/// Oldest schema this build still reads.
pub const OLDEST_READABLE_LOCKFILE_VERSION: u8 = 1;

/// How a lockfile is encoded on disk. Decided by the file's first byte,
/// never by its version: a lockfile that has been mangled into the wrong
/// syntax must be reported as such, not parsed as whatever it claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WireFormat {
    Toml,
    Json,
}

impl WireFormat {
    fn detect(raw: &str) -> Self {
        if raw.trim_start().starts_with('{') {
            Self::Json
        } else {
            Self::Toml
        }
    }

    /// The encoding a schema version is defined in.
    fn for_version(version: u8) -> Self {
        if version >= 3 { Self::Json } else { Self::Toml }
    }
}

impl std::fmt::Display for WireFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Toml => "TOML",
            Self::Json => "JSON",
        })
    }
}

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
    /// A release chosen by tag resolution (RFC-101): `version` is the tag's
    /// semver and `source.tag` names the tag.
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

    /// What kind of string `version` is, given where it came from (RFC-101).
    /// A git install chosen by a release tag is `semver`; one whose version
    /// is the pinned commit itself is `sha`; anything else, including a git
    /// install carrying the version its manifest or frontmatter declared,
    /// is `declared`.
    pub fn version_scheme_for(&self, version: &str) -> VersionScheme {
        match self {
            Self::Git(git) if git.tag.is_some() => VersionScheme::Semver,
            Self::Git(git) if git.git_ref == version => VersionScheme::Sha,
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
    /// The catalog entry id: a built-in id, or the server's full registry
    /// name when `registry` is set.
    pub id: String,
    /// That entry's version at install time.
    pub version: String,
    /// The MCP registry this entry came from, when it did not come from the
    /// catalog compiled into the binary.
    ///
    /// Optional so a built-in install writes exactly what it always wrote:
    /// an older Tuff reading a newer lockfile ignores the field rather than
    /// failing to parse the row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
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

/// Read a lockfile that may legitimately not exist.
///
/// `Ok(None)` means "no lockfile here", which is normal for the global
/// scope on a machine that has never used `--global`. Anything else, in
/// particular a corrupt or too-new file, is an error: reporting it as
/// "nothing installed" would be a confident wrong answer.
pub fn read_optional_lockfile(path: &Path) -> Result<Option<Lockfile>> {
    match read_lockfile_at(path) {
        Ok(lockfile) => Ok(Some(lockfile)),
        Err(error) if error.kind() == crate::error::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Read a lockfile of any supported schema version into the current model.
///
/// The version is read before anything else is deserialised, so a file from
/// a newer tuff fails with a message about versions rather than a shape
/// error naming some field the reader has never heard of.
pub fn read_lockfile_at(path: &Path) -> Result<Lockfile> {
    if !path.exists() {
        let parent = path.parent().unwrap_or(Path::new("."));
        return Err(TuffError::not_found(format!(
            "{} is missing",
            parent
                .join(path.file_name().unwrap_or(OsStr::new("tuff.lock")))
                .display()
        ))
        .with_hint("run 'tuff init' first"));
    }
    let raw = std::fs::read_to_string(path)?;
    let format = WireFormat::detect(&raw);
    let version = peek_version(&raw, format, path)?;
    if version > LOCKFILE_VERSION {
        return Err(TuffError::unsupported(format!(
            "unsupported lockfile version: {version} ({} was written by a newer tuff; this tuff {} reads versions {OLDEST_READABLE_LOCKFILE_VERSION} to {LOCKFILE_VERSION}, upgrade tuff)",
            path.display(),
            env!("CARGO_PKG_VERSION")
        )));
    }
    let expected = WireFormat::for_version(version);
    if format != expected {
        return Err(TuffError::corrupt(format!(
            "{} declares lockfile version {version}, which is {expected}, but the file is {format}",
            path.display()
        )));
    }
    let rows: Vec<Row> = match version {
        1 => read_v1_rows(&raw)?,
        2 => read_v2_rows(&raw)?,
        _ => read_v3_rows(&raw)?,
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

fn peek_version(raw: &str, format: WireFormat, path: &Path) -> Result<u8> {
    #[derive(Deserialize)]
    struct VersionOnly {
        version: Option<u8>,
    }
    let invalid = |message: String| {
        TuffError::corrupt(format!(
            "{} is not a valid lockfile: {message}",
            path.display()
        ))
    };
    let peek: VersionOnly = match format {
        WireFormat::Toml => {
            toml::from_str(raw).map_err(|error| invalid(error.message().to_string()))?
        }
        WireFormat::Json => {
            serde_json::from_str(raw).map_err(|error| invalid(error.to_string()))?
        }
    };
    match peek.version {
        Some(version) if version >= OLDEST_READABLE_LOCKFILE_VERSION => Ok(version),
        Some(version) => Err(TuffError::unsupported(format!(
            "unsupported lockfile version: {version} ({} predates every schema this tuff reads)",
            path.display()
        ))),
        None => Err(TuffError::corrupt(format!(
            "{} has no version field; it is not a Tuff lockfile or it is corrupt",
            path.display()
        ))),
    }
}

/// Schema version 1, read for migration only (RFC-105 D5). Never written.
fn read_v1_rows(raw: &str) -> Result<Vec<Row>> {
    let wire: WireLockfileV1 = toml::from_str(raw)
        .map_err(|error| TuffError::corrupt(format!("invalid version 1 lockfile: {error}")))?;
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
                    // A v1 lockfile predates registry installs, so every
                    // catalog row in one came from the built-in catalog.
                    "catalog" => CapabilitySource::Catalog(CatalogSource {
                        id: item.source_path,
                        version: item.resolved_ref,
                        registry: None,
                    }),
                    // The generated capability index wrote a sentinel path
                    // in v1; it has no source tree and v2 says so plainly.
                    _ if item.source_path == "<generated>" => CapabilitySource::local(""),
                    _ => CapabilitySource::local(item.source_path),
                },
            };
            let version_scheme = source.version_scheme_for(&item.version);
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

/// Schema version 2: the current rows, TOML-encoded. Read for migration
/// only; never written.
fn read_v2_rows(raw: &str) -> Result<Vec<Row>> {
    let wire: WireLockfile = toml::from_str(raw)
        .map_err(|error| TuffError::corrupt(format!("invalid lockfile: {error}")))?;
    Ok(rows_from_wire(wire))
}

/// Schema version 3: the current rows, JSON-encoded.
fn read_v3_rows(raw: &str) -> Result<Vec<Row>> {
    let wire: WireLockfile = serde_json::from_str(raw)
        .map_err(|error| TuffError::corrupt(format!("invalid lockfile: {error}")))?;
    Ok(rows_from_wire(wire))
}

fn rows_from_wire(wire: WireLockfile) -> Vec<Row> {
    wire.capabilities
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
        .collect()
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
    // `to_string_pretty` is the `JSON.stringify(value, null, 2)` layout;
    // the trailing newline is the one thing it leaves out. See
    // `LOCKFILE_VERSION` for why this layout and no other.
    let mut content = serde_json::to_string_pretty(&wire)?;
    content.push('\n');
    std::fs::write(path, content)?;
    Ok(())
}

/// The current rows (RFC-105 D1): one per capability per target. Written
/// as JSON since schema version 3; version 2 was the same rows in TOML,
/// which is why scalars come first and tables after, so that serializer
/// never had to emit a value beneath a table. JSON has no such constraint,
/// but the order is the field order readers of the file expect.
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
    fn read_lockfile_at_rejects_a_newer_schema_in_either_encoding() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tuff.lock");
        for raw in [
            "{\n  \"version\": 4,\n  \"capabilities\": []\n}\n",
            "version = 4\ncapabilities = []\n",
        ] {
            fs::write(&path, raw).unwrap();
            let error = read_lockfile_at(&path).unwrap_err().to_string();
            assert!(error.contains("unsupported lockfile version: 4"), "{error}");
        }
    }

    #[test]
    fn a_lockfile_in_the_wrong_encoding_for_its_version_is_corrupt() {
        // Version 3 is JSON and versions 1 and 2 are TOML; a file claiming
        // one in the syntax of the other was rewritten by something that
        // is not tuff, and the message says which way round it is.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tuff.lock");
        fs::write(&path, "version = 3\ncapabilities = []\n").unwrap();
        let error = read_lockfile_at(&path).unwrap_err().to_string();
        assert!(
            error.contains("declares lockfile version 3, which is JSON, but the file is TOML"),
            "{error}"
        );
        fs::write(&path, "{\"version\": 2, \"capabilities\": []}\n").unwrap();
        let error = read_lockfile_at(&path).unwrap_err().to_string();
        assert!(
            error.contains("declares lockfile version 2, which is TOML, but the file is JSON"),
            "{error}"
        );
    }

    #[test]
    fn an_empty_lockfile_is_canonical_json() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tuff.lock");
        init_lockfile_at(&path).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\n  \"version\": 3,\n  \"capabilities\": []\n}\n"
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
        assert_eq!(read.version, LOCKFILE_VERSION);

        // The layout is the one `JSON.stringify(value, null, 2)` produces:
        // two-space indentation, nothing trailing, one newline at the end.
        let written = fs::read_to_string(&path).unwrap();
        assert!(
            written.starts_with(
                "{\n  \"version\": 3,\n  \"capabilities\": [\n    {\n      \"name\": \"test\",\n"
            ),
            "{written}"
        );
        assert!(written.ends_with("\n  ]\n}\n"), "{written}");
        for line in written.lines() {
            let indent = line.len() - line.trim_start_matches(' ').len();
            assert_eq!(indent % 2, 0, "odd indentation: {line:?}");
            assert!(!line.contains('\t'), "tab in {line:?}");
            assert_eq!(line, line.trim_end(), "trailing whitespace in {line:?}");
        }
        serde_json::from_str::<serde_json::Value>(&written).unwrap();
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
                registry: None,
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

        // Writing produces v3, and v3 round-trips byte for byte.
        write_lockfile_at(&path, &lf).unwrap();
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.starts_with("{\n  \"version\": 3,\n"), "{written}");
        assert!(written.contains("\"kind\": \"pack\""), "{written}");
        assert!(!written.contains("resolved_ref"));
        let again = read_lockfile_at(&path).unwrap();
        assert_eq!(again.version, 3);
        write_lockfile_at(&path, &again).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), written);
    }

    #[test]
    fn a_version_2_lockfile_is_read_as_is_and_written_as_version_3() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tuff.lock");
        fs::write(
            &path,
            r#"version = 2

[[capabilities]]
name = "git-skill"
type = "skill"
version = "1.4.0"
version_scheme = "semver"
target = "open-agents"
installed_path = ".agents/skills/git-skill"
sha256 = "aa"
ownership = "generated"

[capabilities.source]
kind = "git"
url = "https://example.com/skills.git"
path = "skills/git-skill"
ref = "9b9c499"
tag = "v1.4.0"
requested = "^1.2"
"#,
        )
        .unwrap();
        let lf = read_lockfile_at(&path).unwrap();
        assert_eq!(lf.version, 2, "the version read is reported, not rewritten");
        let entry = &lf.capabilities["git-skill"];
        assert_eq!(entry.version_scheme, VersionScheme::Semver);
        assert_eq!(
            entry.source,
            CapabilitySource::Git(GitSource {
                url: "https://example.com/skills.git".into(),
                path: "skills/git-skill".into(),
                git_ref: "9b9c499".into(),
                tag: Some("v1.4.0".into()),
                requested: Some("^1.2".into()),
            })
        );

        write_lockfile_at(&path, &lf).unwrap();
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.starts_with("{\n  \"version\": 3,\n"), "{written}");
        assert!(written.contains("\"requested\": \"^1.2\""), "{written}");
        let again = read_lockfile_at(&path).unwrap();
        assert_eq!(again.version, 3);
        assert_eq!(again.capabilities["git-skill"].source, entry.source);
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
