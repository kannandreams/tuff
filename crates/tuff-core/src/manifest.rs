use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, TuffError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CapabilityType {
    Skill,
    Tool,
    Hook,
    Workflow,
    Policy,
    /// An external MCP server Tuff wires into each harness's native MCP
    /// config. Distinct from a `tool` with `implementation.mcp = true`,
    /// whose server code Tuff ships itself.
    #[serde(rename = "mcp-server")]
    McpServer,
}

impl CapabilityType {
    pub fn plural_dir(&self) -> &'static str {
        match self {
            Self::Skill => "skills",
            Self::Tool => "tools",
            Self::Hook => "hooks",
            Self::Workflow => "workflows",
            Self::Policy => "policies",
            Self::McpServer => "mcp-servers",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Tool => "tool",
            Self::Hook => "hook",
            Self::Workflow => "workflow",
            Self::Policy => "policy",
            Self::McpServer => "mcp-server",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "skill" => Some(Self::Skill),
            "tool" => Some(Self::Tool),
            "hook" => Some(Self::Hook),
            "workflow" => Some(Self::Workflow),
            "policy" => Some(Self::Policy),
            "mcp-server" | "mcp" => Some(Self::McpServer),
            _ => None,
        }
    }
}

impl std::fmt::Display for CapabilityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityManifest {
    pub id: String,
    pub version: String,
    #[serde(rename = "type")]
    pub capability_type: CapabilityType,
    pub description: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub parameters: Option<serde_json::Value>,
    #[serde(default)]
    pub implementation: Option<ImplementationConfig>,
    #[serde(default)]
    pub hook: Option<HookConfig>,
    #[serde(default)]
    pub workflow: Option<WorkflowConfig>,
    #[serde(default)]
    pub server: Option<McpServerConfig>,
    /// The rules of a `type = "policy"` capability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<crate::policy::PolicyConfig>,
    #[serde(default)]
    #[allow(dead_code)]
    pub targets: Vec<String>,

    #[serde(skip)]
    pub root: PathBuf,
}

/// Declaration of an external MCP server (`type = "mcp-server"`).
///
/// Secrets never appear here: every `[server.env]` value must be an
/// [`EnvRef`], and every `[server.headers]` value a [`HeaderRef`], naming
/// the variable to read on the developer's machine, so a manifest can be
/// committed and shared without leaking anything.
#[derive(Debug, Clone, Serialize, Deserialize)]
// The `Option` fields are skipped when absent. TOML has no null and its
// serializer drops them anyway; JSON has one, and the lockfile is JSON, so
// without the skip an absent `url` would be written as `"url": null`.
pub struct McpServerConfig {
    #[serde(default)]
    pub transport: McpTransport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, EnvRef>,
    /// HTTP request headers, keyed by header name. Skipped when empty so a
    /// server that declares none serializes byte-for-byte as it did before
    /// headers existed, and no installed record drifts on upgrade.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub headers: std::collections::BTreeMap<String, HeaderRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<McpServerMetadata>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    #[default]
    Stdio,
    Http,
}

impl McpTransport {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
        }
    }
}

/// A reference to an environment variable on the machine running the
/// harness. Deliberately the only shape an env value can take — a bare
/// string literal is rejected at parse time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvRef {
    pub from_env: String,
}

/// A reference to the environment variable holding one HTTP header's value.
///
/// Headers carry secrets at least as often as environment variables do, so
/// they take the same reference-only shape: a literal string is rejected at
/// parse time. `format` wraps the value, with `{}` standing for it, which
/// is what `Authorization = "Bearer <token>"` needs; it defaults to the
/// bare value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderRef {
    pub from_env: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

impl HeaderRef {
    /// The header value to emit, given how this harness spells a reference
    /// to the variable. The reference is substituted into `format`, so the
    /// harness expands the variable and Tuff never sees the secret.
    pub fn render(&self, value_reference: &str) -> String {
        match &self.format {
            Some(format) => format.replacen(FORMAT_PLACEHOLDER, value_reference, 1),
            None => value_reference.to_string(),
        }
    }
}

/// The one substitution `format` understands.
pub const FORMAT_PLACEHOLDER: &str = "{}";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpServerMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationConfig {
    pub language: String,
    pub entrypoint: String,
    #[serde(default)]
    pub mcp: bool,
    #[serde(default)]
    pub runtime_deps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    pub event: String,
    pub command: String,
    #[serde(default = "default_cwd")]
    pub working_directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    pub requires: Vec<Requirement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirement {
    pub id: String,
    #[serde(rename = "type")]
    pub capability_type: CapabilityType,
}

fn default_cwd() -> String {
    ".".to_string()
}

impl CapabilityManifest {
    /// The files this capability installs, each confirmed to be a regular
    /// file inside the capability directory.
    ///
    /// Every entry in `files`, and a tool's entrypoint, is a path the
    /// manifest's author chose, and a capability can come from anyone's
    /// repository. Each is copied into the project under the harness
    /// directory using the same relative path, so an entry that climbs out
    /// with `..`, is absolute, or passes through a symbolic link would read
    /// a file from outside the capability and write it outside the place
    /// Tuff installs to. Those are refused, the same way pack members and
    /// skill directories already refuse them.
    pub fn source_files(&self) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();

        for f in &self.files {
            let path = contained_source_file(&self.root, f)?;
            paths.push(path);
        }

        if self.capability_type == CapabilityType::Tool
            && let Some(ref imp) = self.implementation
        {
            let ep_path = self.root.join(imp.entrypoint.trim_start_matches("./"));
            if !paths.contains(&ep_path) && ep_path.exists() {
                paths.push(contained_source_file(&self.root, &imp.entrypoint)?);
            }
        }

        Ok(paths)
    }

    /// Each listed file with the path it installs under: its path in the
    /// capability directory with one leading `src/` removed.
    ///
    /// The same file listed twice installs once. Two different files that
    /// would install under the same path are refused: `check.sh` and
    /// `src/check.sh` both install as `check.sh`, and writing both used to
    /// keep whichever came last without saying so.
    pub fn read_source_contents_with_names(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let mut installed: Vec<(String, PathBuf, Vec<u8>)> = Vec::new();
        for p in self.source_files()? {
            let rel = p
                .strip_prefix(&self.root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            let rel = rel.strip_prefix("src/").unwrap_or(&rel).to_string();
            if let Some((_, first, _)) = installed.iter().find(|(name, _, _)| *name == rel) {
                if *first == p {
                    continue;
                }
                return Err(TuffError::refused(format!(
                    "capability '{}' lists two files that would install as '{rel}': {} and {}",
                    self.id,
                    first.display(),
                    p.display()
                ))
                .with_hint("rename one of them, or list only one"));
            }
            let content = std::fs::read(&p)?;
            installed.push((rel, p, content));
        }
        Ok(installed
            .into_iter()
            .map(|(rel, _, content)| (rel, content))
            .collect())
    }
}

/// Resolve one manifest-listed path to a regular file inside `root`.
///
/// The entry must be relative, may start with `./`, and may not contain
/// `..`, a root, or a platform prefix. No component along it may be a
/// symbolic link, since a link is the other way to reach outside the
/// directory while every component still looks plain.
fn contained_source_file(root: &Path, entry: &str) -> Result<PathBuf> {
    let trimmed = entry.trim_start_matches("./");
    let relative = crate::pack::validate_relative_path(Path::new(trimmed)).map_err(|_| {
        TuffError::refused(format!(
            "capability source path must stay inside the capability directory: '{entry}'"
        ))
        .with_hint("list files by their path relative to tuff.toml, without '..' or a leading '/'")
    })?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(TuffError::refused(format!(
                    "symbolic links are not allowed in capability sources: {}",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(_) => {
                return Err(TuffError::not_found(format!(
                    "capability source file not found: {}",
                    root.join(&relative).display()
                )));
            }
        }
    }
    if !current.is_file() {
        return Err(TuffError::usage(format!(
            "capability source must be a file, not a directory: {}",
            current.display()
        )));
    }
    Ok(current)
}

/// Refuse a capability id that could name a directory outside where it
/// belongs.
///
/// The id names the directory a capability is installed into and, when it
/// is deleted, the directory Tuff removes: `<harness>/<kind>s/<id>`. Ids may
/// be nested, `security/security-review` installs into a grouping directory
/// and that is a supported layout, so the rule is not "no slashes". It is
/// that every segment is a plain name: no `..` or `.`, no empty segment from
/// a leading, trailing, or doubled `/`, no backslash, and no NUL. An id that
/// breaks it would aim install and delete outside that directory, and a
/// manifest from someone else's repository, or a lockfile committed to one,
/// could then make `tuff delete` remove a directory outside the project.
/// Every id Tuff accepts goes through here: manifest ids, name overrides,
/// pack artifact members, and every name read back from a lockfile.
pub fn validate_capability_id(id: &str) -> Result<()> {
    let plain = !id.is_empty()
        && id.trim() == id
        && !id.contains(['\\', '\0'])
        && id
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..");
    if plain {
        return Ok(());
    }
    Err(TuffError::refused(format!(
        "capability id must be a relative path of plain names: '{}'",
        id.escape_debug()
    ))
    .with_hint(
        "use a name such as 'release-checklist' or 'security/review', without '..', '.', a leading '/', or '\\'",
    ))
}

fn validate_non_empty(field: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(TuffError::usage(format!(
            "capability manifest field '{field}' must be a non-empty string"
        )));
    }
    Ok(())
}

pub fn load_manifest(capability_dir: &Path) -> Result<CapabilityManifest> {
    let manifest_path = capability_dir.join("tuff.toml");
    if !manifest_path.exists() {
        return Err(TuffError::not_found(format!(
            "capability manifest not found: {}",
            manifest_path.display()
        )));
    }

    let raw = std::fs::read_to_string(&manifest_path)?;
    let mut manifest = parse_manifest(&raw, &manifest_path)?;
    manifest.root = capability_dir.to_path_buf();

    validate_non_empty("id", &manifest.id)?;
    validate_capability_id(&manifest.id)?;
    validate_non_empty("version", &manifest.version)?;
    validate_non_empty("type", &manifest.capability_type.to_string())?;
    validate_non_empty("description", &manifest.description)?;

    match manifest.capability_type {
        CapabilityType::Skill => {
            if manifest.files.is_empty() {
                return Err(TuffError::usage(
                    "skill capability 'files' must not be empty",
                ));
            }
            manifest.source_files()?;
        }
        CapabilityType::Tool => {
            if manifest.parameters.is_none() {
                return Err(TuffError::usage(
                    "tool capability requires a [parameters] section with JSON Schema",
                ));
            }
            if manifest.implementation.is_none() {
                return Err(TuffError::usage(
                    "tool capability requires an [implementation] section",
                ));
            }

            let params = manifest.parameters.as_ref().unwrap();
            crate::tool::validate_json_schema(params)?;

            let impl_cfg = manifest.implementation.as_ref().unwrap();
            crate::tool::validate_entrypoint(&manifest.root, &impl_cfg.entrypoint)?;

            if !impl_cfg.runtime_deps.is_empty() {
                eprintln!(
                    "note: this tool requires runtime dependencies: {}",
                    impl_cfg.runtime_deps.join(", ")
                );
            }

            if !manifest.files.is_empty() {
                manifest.source_files()?;
            }
        }
        CapabilityType::Hook => {
            let hook_cfg = manifest
                .hook
                .as_ref()
                .ok_or_else(|| TuffError::usage("hook capability requires a [hook] section"))?;

            if hook_cfg.event.trim().is_empty() {
                return Err(TuffError::usage("hook 'event' must be a non-empty string"));
            }
            if hook_cfg.command.trim().is_empty() {
                return Err(TuffError::usage(
                    "hook 'command' must be a non-empty string",
                ));
            }

            crate::tool::check_path_traversal(&hook_cfg.working_directory)?;

            eprintln!(
                "note: this hook runs '{}' on event '{}' — it will not be executed during install",
                hook_cfg.command, hook_cfg.event
            );

            if !manifest.files.is_empty() {
                manifest.source_files()?;
            }
        }
        CapabilityType::Workflow => {
            let wf = manifest.workflow.as_ref().ok_or_else(|| {
                TuffError::usage("workflow capability requires a [[workflow.requires]] section")
            })?;

            if wf.requires.is_empty() {
                return Err(TuffError::usage(
                    "workflow 'requires' must have at least one entry",
                ));
            }

            let mut seen = std::collections::HashSet::new();
            for req in &wf.requires {
                if req.id.trim().is_empty() {
                    return Err(TuffError::usage(
                        "workflow requirement 'id' must not be empty",
                    ));
                }
                if req.id == manifest.id {
                    return Err(TuffError::usage("workflow cannot require itself"));
                }
                if !seen.insert(&req.id) {
                    return Err(TuffError::usage(format!(
                        "duplicate requirement '{}' in workflow",
                        req.id
                    )));
                }
            }

            let names: Vec<_> = wf
                .requires
                .iter()
                .map(|r| format!("{} ({})", r.id, r.capability_type))
                .collect();
            eprintln!(
                "note: workflow '{}' requires {} capabilities: {}",
                manifest.id,
                names.len(),
                names.join(", ")
            );
        }
        CapabilityType::Policy => {
            let policy = manifest.policy.as_ref().ok_or_else(|| {
                TuffError::usage(
                    "policy capability requires a [policy] section with at least one [[policy.rules]] entry",
                )
            })?;
            crate::policy::validate_policy(policy)?;
            if !manifest.files.is_empty() {
                return Err(TuffError::usage(
                    "a policy capability installs no files; remove `files` from its tuff.toml",
                ));
            }
        }
        CapabilityType::McpServer => {
            let server = manifest.server.as_ref().ok_or_else(|| {
                TuffError::usage("mcp-server capability requires a [server] section")
            })?;
            validate_mcp_server(server)?;

            if !manifest.files.is_empty() {
                manifest.source_files()?;
            }
        }
    }

    Ok(manifest)
}

pub fn validate_mcp_server(server: &McpServerConfig) -> Result<()> {
    match server.transport {
        McpTransport::Stdio => {
            if server
                .command
                .as_deref()
                .is_none_or(|c| c.trim().is_empty())
            {
                return Err(TuffError::usage(
                    "mcp-server with transport = \"stdio\" requires a non-empty 'command'",
                ));
            }
        }
        McpTransport::Http => {
            if server.url.as_deref().is_none_or(|u| u.trim().is_empty()) {
                return Err(TuffError::usage(
                    "mcp-server with transport = \"http\" requires a non-empty 'url'",
                ));
            }
        }
    }
    for (name, reference) in &server.env {
        if name.trim().is_empty() {
            return Err(TuffError::usage("[server.env] keys must be non-empty"));
        }
        if reference.from_env.trim().is_empty() {
            return Err(TuffError::usage(format!(
                "[server.env] {name} must reference a variable: {name} = {{ from_env = \"VAR\" }}"
            )));
        }
    }
    validate_mcp_headers(server)?;
    Ok(())
}

/// Headers belong to the request a harness makes, so they are meaningful
/// only over HTTP; on a stdio server they would be silently dropped, which
/// is exactly the quiet-success failure RFC-106 exists to remove.
fn validate_mcp_headers(server: &McpServerConfig) -> Result<()> {
    if !server.headers.is_empty() && server.transport != McpTransport::Http {
        return Err(TuffError::usage(
            "[server.headers] applies to transport = \"http\"; a stdio server passes \
             secrets through [server.env]",
        ));
    }
    for (name, reference) in &server.headers {
        if name.trim().is_empty() {
            return Err(TuffError::usage("[server.headers] keys must be non-empty"));
        }
        if reference.from_env.trim().is_empty() {
            return Err(TuffError::usage(format!(
                "[server.headers] {name} must reference a variable: \
                 {name} = {{ from_env = \"VAR\" }}"
            )));
        }
        if let Some(format) = &reference.format {
            let placeholders = format.matches(FORMAT_PLACEHOLDER).count();
            if placeholders != 1 {
                let problem = if placeholders == 0 {
                    "would discard the value"
                } else {
                    "would repeat the value"
                };
                return Err(TuffError::usage(format!(
                    "[server.headers] {name}: format \"{format}\" {problem}"
                ))
                .with_hint("format must contain exactly one {} placeholder, as in \"Bearer {}\""));
            }
        }
    }
    Ok(())
}

/// Parse a manifest, turning serde's opaque "invalid type: string" failure
/// for a literal `[server.env]` value into an error that says what to write
/// instead.
fn parse_manifest(raw: &str, manifest_path: &Path) -> Result<CapabilityManifest> {
    toml::from_str(raw).map_err(|error: toml::de::Error| {
        let message = error.to_string();
        let looks_literal =
            message.contains("invalid type: string") || message.contains("expected a table");
        let literal_table = looks_literal
            .then(|| {
                ["[server.env]", "[server.headers]"]
                    .into_iter()
                    .find(|table| raw.contains(table))
            })
            .flatten();
        if let Some(table) = literal_table {
            let example = if table == "[server.headers]" {
                "Authorization = { from_env = \"TOKEN\", format = \"Bearer {}\" }"
            } else {
                "NAME = { from_env = \"NAME\" }"
            };
            TuffError::usage(format!(
                "invalid manifest at {}: {} values must be references, never \
                 literals — write {} ({})",
                manifest_path.display(),
                table,
                example,
                message.trim()
            ))
        } else {
            TuffError::from(error)
        }
    })
}

/// Writes a capability manifest as deterministic TOML.
///
/// # Errors
///
/// Returns an error when serialization or filesystem writing fails.
pub fn write_manifest(path: &Path, manifest: &CapabilityManifest) -> Result<()> {
    std::fs::write(path, toml::to_string_pretty(manifest)?)?;
    Ok(())
}

/// The version a capability source declares for itself, if any: `version`
/// in `tuff.toml`, else `version:` or `metadata.version:` in the `SKILL.md`
/// frontmatter (RFC-101 tier 2). It is what the author wrote, not what was
/// released: it may not change when the content does, which is why a
/// release tag outranks it and the lockfile records which one it holds.
pub fn declared_version(dir: &Path) -> Option<String> {
    if dir.join("tuff.toml").is_file() {
        return load_manifest(dir).ok().map(|manifest| manifest.version);
    }
    let skill = std::fs::read_to_string(dir.join("SKILL.md")).ok()?;
    frontmatter_version(&skill)
}

/// Read a version out of `SKILL.md` frontmatter without a YAML parser: a
/// top-level `version:` line, else `version:` indented under `metadata:`,
/// which is where the Agent Skills specification puts it. Quotes are
/// stripped; anything else is taken as written.
pub fn frontmatter_version(skill: &str) -> Option<String> {
    let mut lines = skill.lines().map(|line| line.trim_end_matches('\r'));
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut in_metadata = false;
    let mut nested = None;
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        let indented = line.starts_with([' ', '\t']);
        if !indented {
            in_metadata = line.trim_end() == "metadata:";
            if let Some(value) = line.strip_prefix("version:") {
                return frontmatter_scalar(value);
            }
            continue;
        }
        if in_metadata
            && nested.is_none()
            && let Some(value) = line.trim_start().strip_prefix("version:")
        {
            nested = frontmatter_scalar(value);
        }
    }
    nested
}

/// The description a capability source declares for itself, if any:
/// `description` in `tuff.toml`, else `description:` in the `SKILL.md`
/// frontmatter, which is where the Agent Skills specification puts it.
///
/// Unlike a version, a description is prose that no command depends on, so
/// an absent one is an empty line in a report rather than an error.
pub fn declared_description(dir: &Path) -> Option<String> {
    if dir.join("tuff.toml").is_file() {
        return load_manifest(dir).ok().map(|manifest| manifest.description);
    }
    let skill = std::fs::read_to_string(dir.join("SKILL.md")).ok()?;
    frontmatter_description(&skill)
}

/// Read a top-level `description:` out of `SKILL.md` frontmatter.
///
/// Only the top level, and only a single line: a folded or block scalar is
/// left alone rather than half-read, because a description this misses is a
/// blank cell, while one it mangles is a wrong cell.
pub fn frontmatter_description(skill: &str) -> Option<String> {
    let mut lines = skill.lines().map(|line| line.trim_end_matches('\r'));
    if lines.next()?.trim() != "---" {
        return None;
    }
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if line.starts_with([' ', '\t']) {
            continue;
        }
        if let Some(value) = line.strip_prefix("description:") {
            return frontmatter_text(value);
        }
    }
    None
}

/// A frontmatter value that is allowed to contain spaces, unlike a version.
fn frontmatter_text(value: &str) -> Option<String> {
    let value = value.trim();
    let value = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .unwrap_or(value)
        .trim();
    // `>` and `|` open a multi-line scalar whose body is on the next lines.
    if value.is_empty() || value.starts_with(['>', '|']) {
        return None;
    }
    Some(value.to_string())
}

fn frontmatter_scalar(value: &str) -> Option<String> {
    let value = value.trim();
    let value = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .unwrap_or(value)
        .trim();
    (!value.is_empty() && !value.contains(char::is_whitespace)).then(|| value.to_string())
}

pub fn synthetic_manifest(
    skill_dir: &Path,
    name: &str,
    version: &str,
) -> Result<CapabilityManifest> {
    validate_capability_id(name)?;
    let skill_file = skill_dir.join("SKILL.md");
    if !skill_file.exists() {
        return Err(TuffError::not_found(format!(
            "skill entrypoint not found: {}",
            skill_file.display()
        )));
    }
    let mut files = Vec::new();
    walk_skill_dir(skill_dir, "", &mut files)?;
    files.sort();

    Ok(CapabilityManifest {
        id: name.to_string(),
        version: version.to_string(),
        capability_type: CapabilityType::Skill,
        description: "Installed from git source.".to_string(),
        files,
        parameters: None,
        implementation: None,
        hook: None,
        workflow: None,
        server: None,
        policy: None,
        targets: Vec::new(),
        root: skill_dir.to_path_buf(),
    })
}

fn walk_skill_dir(base: &Path, prefix: &str, files: &mut Vec<String>) -> Result<()> {
    for entry in std::fs::read_dir(base)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(TuffError::refused(format!(
                "symbolic links are not allowed in capability sources: {}",
                path.display()
            )));
        }
        let rel = if prefix.is_empty() {
            entry.file_name().to_string_lossy().to_string()
        } else {
            format!("{}/{}", prefix, entry.file_name().to_string_lossy())
        };
        if metadata.is_dir() {
            walk_skill_dir(&path, &rel, files)?;
        } else if metadata.is_file() && rel != "tuff.toml" {
            files.push(rel);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_manifest(dir: &std::path::Path, content: &str) {
        fs::write(dir.join("tuff.toml"), content).unwrap();
    }

    #[test]
    fn frontmatter_version_reads_top_level_then_metadata() {
        assert_eq!(
            frontmatter_version("---\nname: x\nversion: 1.2.0\n---\n# X\n").as_deref(),
            Some("1.2.0")
        );
        assert_eq!(
            frontmatter_version("---\nname: x\nversion: \"1.2.0\"\n---\n").as_deref(),
            Some("1.2.0")
        );
        // The Agent Skills specification nests it under `metadata`.
        assert_eq!(
            frontmatter_version(
                "---\nname: x\nmetadata:\n  author: org\n  version: \"1.0\"\n---\n"
            )
            .as_deref(),
            Some("1.0")
        );
        // Top level wins over nested when both are present.
        assert_eq!(
            frontmatter_version("---\nmetadata:\n  version: 0.9.0\nversion: 1.2.0\n---\n")
                .as_deref(),
            Some("1.2.0")
        );
        // A `version:` nested under some other key is not the skill's.
        assert_eq!(
            frontmatter_version("---\nname: x\nextra:\n  version: 3.0.0\n---\n"),
            None
        );
        assert_eq!(
            frontmatter_version("# no frontmatter\nversion: 1.0.0\n"),
            None
        );
        assert_eq!(
            frontmatter_version("---\nname: x\n---\nversion: 9.9.9\n"),
            None
        );
        assert_eq!(frontmatter_version("---\nversion:\n---\n"), None);
        assert_eq!(frontmatter_version("---\nversion: 1.2.0 beta\n---\n"), None);
        assert_eq!(
            frontmatter_version("---\r\nname: x\r\nversion: 2.0.0\r\n---\r\n").as_deref(),
            Some("2.0.0")
        );
    }

    #[test]
    fn declared_description_reads_the_frontmatter_and_prefers_the_manifest() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("SKILL.md"),
            "---\nname: x\ndescription: Reviews a diff for security problems.\n---\n# X\n",
        )
        .unwrap();
        assert_eq!(
            declared_description(tmp.path()).as_deref(),
            Some("Reviews a diff for security problems.")
        );

        write_manifest(
            tmp.path(),
            r#"id = "x"
version = "1.0.0"
type = "skill"
description = "From the manifest"
files = ["SKILL.md"]
"#,
        );
        assert_eq!(
            declared_description(tmp.path()).as_deref(),
            Some("From the manifest")
        );
    }

    #[test]
    fn a_multi_line_description_is_left_alone_rather_than_half_read() {
        // A folded scalar's text is on the following lines, so reading the
        // marker would record ">" as the description.
        assert_eq!(
            frontmatter_description("---\nname: x\ndescription: >\n  Long text here.\n---\n"),
            None
        );
        // An indented `description:` belongs to some nested mapping, not to
        // the skill.
        assert_eq!(
            frontmatter_description("---\nmetadata:\n  description: Nested.\n---\n"),
            None
        );
        // No frontmatter at all is not an error.
        assert_eq!(frontmatter_description("# Just a heading\n"), None);
    }

    #[test]
    fn declared_version_prefers_the_manifest_over_the_frontmatter() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("SKILL.md"),
            "---\nname: x\nversion: 2.0.0\n---\n# X\n",
        )
        .unwrap();
        assert_eq!(declared_version(tmp.path()).as_deref(), Some("2.0.0"));
        fs::write(
            tmp.path().join("tuff.toml"),
            "id = \"x\"\nversion = \"1.0.0\"\ntype = \"skill\"\ndescription = \"d\"\nfiles = [\"SKILL.md\"]\n",
        )
        .unwrap();
        assert_eq!(declared_version(tmp.path()).as_deref(), Some("1.0.0"));

        let bare = TempDir::new().unwrap();
        fs::write(bare.path().join("SKILL.md"), "# no version\n").unwrap();
        assert_eq!(declared_version(bare.path()), None);
    }

    #[test]
    fn load_skill_manifest_succeeds() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src").join("SKILL.md"), "# Skill").unwrap();
        write_manifest(
            tmp.path(),
            r#"id = "test"
version = "1.0.0"
type = "skill"
description = "A test skill"
files = ["src/SKILL.md"]
"#,
        );
        let m = load_manifest(tmp.path()).unwrap();
        assert_eq!(m.id, "test");
        assert_eq!(m.capability_type, CapabilityType::Skill);
    }

    #[test]
    fn load_tool_manifest_succeeds() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("run.sh"), "echo ok").unwrap();
        write_manifest(
            tmp.path(),
            r#"id = "tool1"
version = "1.0.0"
type = "tool"
description = "A test tool"
files = ["run.sh"]

[parameters]
type = "object"
required = ["x"]
[parameters.properties.x]
type = "string"
description = "x"

[implementation]
language = "bash"
entrypoint = "run.sh"
"#,
        );
        let m = load_manifest(tmp.path()).unwrap();
        assert_eq!(m.capability_type, CapabilityType::Tool);
        assert!(m.implementation.is_some());
    }

    #[test]
    fn load_hook_manifest_succeeds() {
        let tmp = TempDir::new().unwrap();
        write_manifest(
            tmp.path(),
            r#"id = "hook1"
version = "1.0.0"
type = "hook"
description = "A test hook"

[hook]
event = "before_finish"
command = "cargo test"
"#,
        );
        let m = load_manifest(tmp.path()).unwrap();
        assert_eq!(m.capability_type, CapabilityType::Hook);
        assert!(m.hook.is_some());
    }

    #[test]
    fn load_rejects_unsupported_type() {
        let tmp = TempDir::new().unwrap();
        write_manifest(
            tmp.path(),
            r#"id = "bad"
version = "1.0.0"
type = "unknown"
description = "Bad"
files = ["SKILL.md"]
"#,
        );
        assert!(load_manifest(tmp.path()).is_err());
    }

    #[test]
    fn load_rejects_missing_manifest() {
        let tmp = TempDir::new().unwrap();
        assert!(load_manifest(tmp.path()).is_err());
    }

    #[test]
    fn source_files_resolves_paths() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src").join("SKILL.md"), "skill").unwrap();
        let m = CapabilityManifest {
            id: "t".into(),
            version: "1.0".into(),
            capability_type: CapabilityType::Skill,
            description: "desc".into(),
            files: vec!["src/SKILL.md".into()],
            parameters: None,
            implementation: None,
            hook: None,
            workflow: None,
            server: None,
            policy: None,
            targets: vec![],
            root: tmp.path().to_path_buf(),
        };
        let files = m.source_files().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("SKILL.md"));
    }

    fn manifest_listing(root: &Path, files: &[&str]) -> CapabilityManifest {
        CapabilityManifest {
            id: "t".into(),
            version: "1.0".into(),
            capability_type: CapabilityType::Hook,
            description: "desc".into(),
            files: files.iter().map(|f| f.to_string()).collect(),
            parameters: None,
            implementation: None,
            hook: None,
            workflow: None,
            server: None,
            policy: None,
            targets: vec![],
            root: root.to_path_buf(),
        }
    }

    #[test]
    fn source_files_refuse_a_path_that_climbs_out_of_the_capability() {
        // A capability from someone else's repository chooses these paths.
        // `../` would read a file beside the capability and, because the
        // relative path is reused for the destination, write it outside the
        // harness directory Tuff installs into.
        let tmp = TempDir::new().unwrap();
        let capability = tmp.path().join("capability");
        fs::create_dir_all(&capability).unwrap();
        fs::write(tmp.path().join("outside.txt"), "outside").unwrap();
        fs::write(capability.join("inside.txt"), "inside").unwrap();

        for entry in [
            "../outside.txt",
            "sub/../../outside.txt",
            "./../outside.txt",
        ] {
            let error = manifest_listing(&capability, &["inside.txt", entry])
                .source_files()
                .unwrap_err();
            assert_eq!(error.kind(), crate::error::ErrorKind::Refused, "{entry}");
            assert!(
                error
                    .to_string()
                    .contains("must stay inside the capability directory"),
                "{entry}: {error}"
            );
        }
        let absolute = tmp.path().join("outside.txt");
        let error = manifest_listing(&capability, &[absolute.to_str().unwrap()])
            .source_files()
            .unwrap_err();
        assert_eq!(error.kind(), crate::error::ErrorKind::Refused);
    }

    #[cfg(unix)]
    #[test]
    fn source_files_refuse_a_symbolic_link_anywhere_along_the_path() {
        let tmp = TempDir::new().unwrap();
        let capability = tmp.path().join("capability");
        let secrets = tmp.path().join("secrets");
        fs::create_dir_all(capability.join("docs")).unwrap();
        fs::create_dir_all(&secrets).unwrap();
        fs::write(secrets.join("key.txt"), "pretend secret").unwrap();
        std::os::unix::fs::symlink(secrets.join("key.txt"), capability.join("notes.md")).unwrap();
        std::os::unix::fs::symlink(&secrets, capability.join("docs").join("linked")).unwrap();

        for entry in ["notes.md", "docs/linked/key.txt"] {
            let error = manifest_listing(&capability, &[entry])
                .source_files()
                .unwrap_err();
            assert_eq!(error.kind(), crate::error::ErrorKind::Refused, "{entry}");
            assert!(
                error.to_string().contains("symbolic links are not allowed"),
                "{entry}: {error}"
            );
        }
    }

    #[test]
    fn source_files_accept_plain_nested_and_dot_slash_paths() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("src/lib")).unwrap();
        fs::write(tmp.path().join("src/lib/check.sh"), "x").unwrap();
        fs::write(tmp.path().join("run.sh"), "x").unwrap();

        let files = manifest_listing(tmp.path(), &["./run.sh", "src/lib/check.sh"])
            .source_files()
            .unwrap();
        assert_eq!(
            files,
            vec![
                tmp.path().join("run.sh"),
                tmp.path().join("src/lib/check.sh")
            ]
        );
        let error = manifest_listing(tmp.path(), &["src"])
            .source_files()
            .unwrap_err();
        assert!(error.to_string().contains("must be a file"), "{error}");
    }

    #[test]
    fn listed_files_that_install_to_the_same_path_are_refused_but_a_repeat_is_not() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("check.sh"), "top").unwrap();
        fs::write(tmp.path().join("src/check.sh"), "src").unwrap();

        let repeated = manifest_listing(tmp.path(), &["check.sh", "./check.sh", "check.sh"])
            .read_source_contents_with_names()
            .unwrap();
        assert_eq!(repeated, vec![("check.sh".to_string(), b"top".to_vec())]);

        let error = manifest_listing(tmp.path(), &["check.sh", "src/check.sh"])
            .read_source_contents_with_names()
            .unwrap_err();
        assert_eq!(error.kind(), crate::error::ErrorKind::Refused);
        assert!(
            error.to_string().contains("would install as 'check.sh'"),
            "{error}"
        );
    }

    #[test]
    fn source_files_rejects_missing_file() {
        let tmp = TempDir::new().unwrap();
        let m = CapabilityManifest {
            id: "t".into(),
            version: "1.0".into(),
            capability_type: CapabilityType::Skill,
            description: "desc".into(),
            files: vec!["src/MISSING.md".into()],
            parameters: None,
            implementation: None,
            hook: None,
            workflow: None,
            server: None,
            policy: None,
            targets: vec![],
            root: tmp.path().to_path_buf(),
        };
        assert!(m.source_files().is_err());
    }

    #[test]
    fn a_capability_id_is_a_relative_path_of_plain_names() {
        // Nested ids are a supported layout (`tuff add skill <repo>
        // security/security-review`), so they must keep working.
        for id in [
            "release-checklist",
            "tuff-cli-guide",
            "a.b",
            "x_1",
            "...x",
            "security/security-review",
            "a/b/c",
        ] {
            assert!(validate_capability_id(id).is_ok(), "{id}");
        }
        for id in [
            "",
            ".",
            "..",
            "../victim",
            "../../victim",
            "a/../b",
            "a/..",
            "./a",
            "a/./b",
            "/abs",
            "a/",
            "a//b",
            "a\\b",
            "nul\0x",
            " padded",
            "padded ",
        ] {
            let error = validate_capability_id(id).unwrap_err();
            assert_eq!(error.kind(), crate::error::ErrorKind::Refused, "{id:?}");
        }
    }

    #[test]
    fn a_manifest_with_an_escaping_id_is_refused_at_load() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("tuff.toml"),
            "id = \"../../victim\"\ntype = \"hook\"\nversion = \"1.0.0\"\ndescription = \"d\"\n[hook]\nevent = \"stop\"\ncommand = \"true\"\n",
        )
        .unwrap();
        let error = load_manifest(tmp.path()).unwrap_err();
        assert!(
            error.to_string().contains("relative path of plain names"),
            "{error}"
        );
    }

    #[test]
    fn a_policy_manifest_loads_and_refuses_files() {
        let tmp = TempDir::new().unwrap();
        let head = "id = \"guard\"\ntype = \"policy\"\nversion = \"1.0.0\"\ndescription = \"d\"\n";
        let rules =
            "[[policy.rules]]\neffect = \"deny\"\ncommand = [\"git\", \"push\", \"--force\"]\n";
        fs::write(tmp.path().join("tuff.toml"), format!("{head}{rules}")).unwrap();
        let manifest = load_manifest(tmp.path()).unwrap();
        assert_eq!(manifest.policy.unwrap().rules.len(), 1);

        fs::write(tmp.path().join("tuff.toml"), head).unwrap();
        let error = load_manifest(tmp.path()).unwrap_err();
        assert!(
            error.to_string().contains("requires a [policy] section"),
            "{error}"
        );

        fs::write(tmp.path().join("x.sh"), "x").unwrap();
        fs::write(
            tmp.path().join("tuff.toml"),
            format!("{head}files = [\"x.sh\"]\n{rules}"),
        )
        .unwrap();
        let error = load_manifest(tmp.path()).unwrap_err();
        assert!(error.to_string().contains("installs no files"), "{error}");
    }

    #[test]
    fn validate_non_empty_rejects_empty() {
        assert!(validate_non_empty("id", "").is_err());
        assert!(validate_non_empty("id", "ok").is_ok());
    }

    fn load_mcp(toml_body: &str) -> Result<CapabilityManifest> {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("tuff.toml"), toml_body).unwrap();
        load_manifest(tmp.path())
    }

    const MCP_HEAD: &str =
        "id = \"srv\"\nversion = \"1.0.0\"\ntype = \"mcp-server\"\ndescription = \"d\"\n";

    #[test]
    fn mcp_server_requires_server_section() {
        let error = load_mcp(MCP_HEAD).unwrap_err().to_string();
        assert!(error.contains("requires a [server] section"), "{error}");
    }

    #[test]
    fn mcp_server_stdio_requires_command_and_http_requires_url() {
        let error = load_mcp(&format!("{MCP_HEAD}[server]\ntransport = \"stdio\"\n"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("requires a non-empty 'command'"), "{error}");
        let error = load_mcp(&format!("{MCP_HEAD}[server]\ntransport = \"http\"\n"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("requires a non-empty 'url'"), "{error}");
        let ok = load_mcp(&format!(
            "{MCP_HEAD}[server]\ntransport = \"http\"\nurl = \"https://example.test/mcp\"\n"
        ))
        .unwrap();
        assert_eq!(ok.server.unwrap().transport, McpTransport::Http);
    }

    #[test]
    fn mcp_server_env_must_be_a_reference_not_a_literal() {
        let error = load_mcp(&format!(
            "{MCP_HEAD}[server]\ncommand = \"npx\"\n[server.env]\nTOKEN = \"literal\"\n"
        ))
        .unwrap_err()
        .to_string();
        assert!(error.contains("from_env"), "{error}");

        let ok = load_mcp(&format!(
            "{MCP_HEAD}[server]\ncommand = \"npx\"\n[server.env]\nTOKEN = {{ from_env = \"MY_TOKEN\" }}\n"
        ))
        .unwrap();
        assert_eq!(ok.server.unwrap().env["TOKEN"].from_env, "MY_TOKEN");
    }

    #[test]
    fn mcp_server_headers_must_be_references_not_literals() {
        let error = load_mcp(&format!(
            "{MCP_HEAD}[server]\ntransport = \"http\"\nurl = \"https://example.test/mcp\"\n\
             [server.headers]\nAuthorization = \"Bearer secret\"\n"
        ))
        .unwrap_err()
        .to_string();
        assert!(error.contains("[server.headers]"), "{error}");
        assert!(error.contains("from_env"), "{error}");
    }

    #[test]
    fn mcp_server_header_reference_carries_an_optional_format() {
        let server = load_mcp(&format!(
            "{MCP_HEAD}[server]\ntransport = \"http\"\nurl = \"https://example.test/mcp\"\n\
             [server.headers]\n\
             Authorization = {{ from_env = \"NOTION_TOKEN\", format = \"Bearer {{}}\" }}\n\
             X-Api-Key = {{ from_env = \"API_KEY\" }}\n"
        ))
        .unwrap()
        .server
        .unwrap();
        assert_eq!(server.headers["Authorization"].from_env, "NOTION_TOKEN");
        assert_eq!(
            server.headers["Authorization"].render("${NOTION_TOKEN}"),
            "Bearer ${NOTION_TOKEN}"
        );
        assert_eq!(server.headers["X-Api-Key"].format, None);
        assert_eq!(
            server.headers["X-Api-Key"].render("${API_KEY}"),
            "${API_KEY}"
        );
    }

    #[test]
    fn mcp_server_header_format_needs_exactly_one_placeholder() {
        for format in ["Bearer", "Bearer {} {}"] {
            let error = load_mcp(&format!(
                "{MCP_HEAD}[server]\ntransport = \"http\"\nurl = \"https://example.test/mcp\"\n\
                 [server.headers]\n\
                 Authorization = {{ from_env = \"TOKEN\", format = \"{format}\" }}\n"
            ))
            .unwrap_err()
            .to_string();
            assert!(error.contains("Authorization"), "{format}: {error}");
        }
    }

    #[test]
    fn mcp_server_headers_are_refused_on_stdio() {
        let error = load_mcp(&format!(
            "{MCP_HEAD}[server]\ncommand = \"npx\"\n\
             [server.headers]\nAuthorization = {{ from_env = \"TOKEN\" }}\n"
        ))
        .unwrap_err()
        .to_string();
        assert!(error.contains("http"), "{error}");
    }

    /// A server without headers has to serialize exactly as it did before
    /// the field existed, or every installed record drifts on upgrade.
    #[test]
    fn a_server_without_headers_serializes_without_the_table() {
        let server = load_mcp(&format!("{MCP_HEAD}[server]\ncommand = \"npx\"\n"))
            .unwrap()
            .server
            .unwrap();
        let wire = toml::to_string_pretty(&server).unwrap();
        assert!(!wire.contains("headers"), "{wire}");
    }

    #[test]
    fn capability_type_round_trips_the_hyphenated_name() {
        assert_eq!(CapabilityType::McpServer.as_str(), "mcp-server");
        assert_eq!(
            CapabilityType::parse("mcp-server"),
            Some(CapabilityType::McpServer)
        );
        assert_eq!(
            CapabilityType::parse("mcp"),
            Some(CapabilityType::McpServer)
        );
        let wire = toml::to_string(&Requirement {
            id: "x".into(),
            capability_type: CapabilityType::McpServer,
        })
        .unwrap();
        assert!(wire.contains("type = \"mcp-server\""), "{wire}");
    }
}
