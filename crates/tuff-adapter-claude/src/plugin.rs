//! Claude Code plugins read as Tuff packs (RFC-110 D11).
//!
//! A plugin is a folder with an optional `.claude-plugin/plugin.json`,
//! skills under `skills/<name>/SKILL.md`, and MCP servers in `.mcp.json`
//! or the manifest's `mcpServers`. A marketplace is a repository with
//! `.claude-plugin/marketplace.json` listing plugins. Skills and MCP servers
//! become pack members. Everything else, commands, agents, hooks, output
//! styles, LSP servers, and the rest, is recorded as unsupported with a
//! reason, and its files are kept under the pack's `extra/` folder.
//!
//! Checked against the Claude Code plugin and marketplace references on
//! 2026-11-26 (code.claude.com/docs/en/plugins-reference and
//! /en/plugin-marketplaces).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use tuff_core::bundle::{
    self, BundleFile, BundleLocation, BundleReader, ImportedMember, ImportedPack, LocalBundle,
    RemoteBundle,
};
use tuff_core::error::{Result, TuffError};
use tuff_core::manifest::{self, CapabilityType, McpServerConfig};
use tuff_core::pack::{PackAuthor, PackBuild, PackImport, PackManifest, PackUnsupported};

/// The bundle format id, used as an output target in a later milestone.
pub const FORMAT_ID: &str = "claude-plugin";
const PLUGIN_MANIFEST: &str = ".claude-plugin/plugin.json";
const MARKETPLACE_MANIFEST: &str = ".claude-plugin/marketplace.json";

/// The Claude Code plugin bundle format.
pub struct ClaudePlugin;

impl BundleReader for ClaudePlugin {
    fn id(&self) -> &'static str {
        FORMAT_ID
    }

    fn detect(&self, root: &Path) -> bool {
        root.join(PLUGIN_MANIFEST).is_file() || root.join(MARKETPLACE_MANIFEST).is_file()
    }

    fn locate(&self, root: &Path, entry: Option<&str>) -> Result<BundleLocation> {
        let plugin_manifest = root.join(PLUGIN_MANIFEST);
        let marketplace = root.join(MARKETPLACE_MANIFEST);
        if entry.is_none() && plugin_manifest.is_file() {
            return Ok(BundleLocation::Local(LocalBundle {
                root: root.to_path_buf(),
                name: None,
                entry: None,
            }));
        }
        if marketplace.is_file() {
            return locate_in_marketplace(root, &read_json(root, MARKETPLACE_MANIFEST)?, entry);
        }
        match entry {
            Some(name) if plugin_manifest.is_file() => {
                let manifest = read_json(root, PLUGIN_MANIFEST)?;
                if manifest.get("name").and_then(Value::as_str) == Some(name) {
                    Ok(BundleLocation::Local(LocalBundle {
                        root: root.to_path_buf(),
                        name: Some(name.to_string()),
                        entry: None,
                    }))
                } else {
                    Err(TuffError::not_found(format!(
                        "this folder holds one plugin, and it is not named '{name}'"
                    )))
                }
            }
            _ => Err(TuffError::not_found(format!(
                "no {PLUGIN_MANIFEST} or {MARKETPLACE_MANIFEST} at {}",
                root.display()
            ))),
        }
    }

    fn read(&self, bundle: &LocalBundle, version_hint: Option<&str>) -> Result<ImportedPack> {
        Reader::new(bundle, version_hint)?.read()
    }
}

/// A plugin's name and the version it declares, if any: what release tags
/// are read against, without reading the whole plugin.
///
/// # Errors
///
/// Returns an error for an unreadable manifest or a name that is not plain.
pub fn identity(bundle: &LocalBundle) -> Result<(String, Option<String>)> {
    let reader = Reader::new(bundle, None)?;
    let declared = reader
        .manifest
        .get("version")
        .and_then(Value::as_str)
        .filter(|version| !version.trim().is_empty())
        .map(str::to_string);
    Ok((reader.name, declared))
}

fn read_json(root: &Path, relative: &str) -> Result<Value> {
    let path = root.join(relative);
    let metadata = std::fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() {
        return Err(TuffError::refused(format!(
            "{relative} is a symbolic link; Tuff does not follow links in a plugin"
        )));
    }
    let raw = std::fs::read_to_string(&path)?;
    serde_json::from_str(&raw)
        .map_err(|error| TuffError::corrupt(format!("{relative} is not valid JSON: {error}")))
}

fn locate_in_marketplace(
    root: &Path,
    listing: &Value,
    entry: Option<&str>,
) -> Result<BundleLocation> {
    let plugins = listing
        .get("plugins")
        .and_then(Value::as_array)
        .ok_or_else(|| TuffError::corrupt(format!("{MARKETPLACE_MANIFEST} has no plugins list")))?;
    let names = plugins
        .iter()
        .filter_map(|plugin| plugin.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    let selected = match entry {
        Some(name) => plugins
            .iter()
            .find(|plugin| plugin.get("name").and_then(Value::as_str) == Some(name))
            .ok_or_else(|| {
                TuffError::not_found(format!(
                    "the marketplace lists no plugin named '{name}'; it lists {}",
                    names.join(", ")
                ))
            })?,
        None => match plugins.as_slice() {
            [only] => only,
            _ => {
                return Err(TuffError::usage(format!(
                    "the marketplace lists {} plugins: {}",
                    plugins.len(),
                    names.join(", ")
                ))
                .with_hint("choose one with --plugin <name>"));
            }
        },
    };
    let name = selected
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| TuffError::corrupt("a marketplace entry has no name"))?
        .to_string();
    let source = selected
        .get("source")
        .ok_or_else(|| TuffError::corrupt(format!("marketplace entry '{name}' has no source")))?;
    match source {
        Value::String(path) => {
            let plugin_root = listing
                .pointer("/metadata/pluginRoot")
                .and_then(Value::as_str)
                .filter(|_| !path.starts_with("./") && path != ".")
                .map(bundle::bundle_path)
                .transpose()?
                .unwrap_or_default();
            let relative = plugin_root.join(bundle::bundle_path(path)?);
            let folder = root.join(&relative);
            if !folder.is_dir() {
                return Err(TuffError::not_found(format!(
                    "marketplace entry '{name}' points to {}, which is not a folder in the repository",
                    relative.display()
                )));
            }
            Ok(BundleLocation::Local(LocalBundle {
                root: folder,
                name: Some(name),
                entry: Some(selected.clone()),
            }))
        }
        Value::Object(source) => {
            let text = |key: &str| source.get(key).and_then(Value::as_str).map(str::to_string);
            let kind = text("source").unwrap_or_default();
            let github = |repo: String| {
                if repo.contains("://") || repo.starts_with("git@") {
                    repo
                } else {
                    format!("https://github.com/{repo}")
                }
            };
            let (url, path) = match kind.as_str() {
                "github" => (text("repo").map(github), None),
                "url" => (text("url"), None),
                "git-subdir" => (text("url").map(github), text("path")),
                other => {
                    return Err(TuffError::unsupported(format!(
                        "marketplace entry '{name}' is installed from a '{other}' source, which Tuff does not read"
                    ))
                    .with_hint("Tuff reads plugins from the marketplace's own repository and from git repositories"));
                }
            };
            let url = url.ok_or_else(|| {
                TuffError::corrupt(format!("marketplace entry '{name}' names no repository"))
            })?;
            Ok(BundleLocation::Remote(RemoteBundle {
                name,
                url,
                path,
                git_ref: text("ref"),
                commit: text("sha"),
                entry: selected.clone(),
            }))
        }
        _ => Err(TuffError::corrupt(format!(
            "marketplace entry '{name}' has a source that is neither a path nor an object"
        ))),
    }
}

/// Folders and manifest keys Tuff does not carry, with why.
const UNSUPPORTED_COMPONENTS: &[(&str, Option<&str>, Option<&str>, &str)] = &[
    (
        "commands",
        Some("commands"),
        Some("commands"),
        "Tuff has no slash-command capability",
    ),
    (
        "agents",
        Some("agents"),
        Some("agents"),
        "Tuff has no subagent capability",
    ),
    (
        "hooks",
        Some("hooks/hooks.json"),
        Some("hooks"),
        "Claude Code hooks have matchers and handler types that a tuff.toml hook cannot express, so hooks are not imported",
    ),
    (
        "output-styles",
        Some("output-styles"),
        Some("outputStyles"),
        "Tuff has no output-style capability",
    ),
    (
        "lsp-servers",
        Some(".lsp.json"),
        Some("lspServers"),
        "Tuff has no LSP server capability",
    ),
    (
        "executables",
        Some("bin"),
        None,
        "Tuff does not put a plugin's programs on the agent's PATH",
    ),
    (
        "workflows",
        Some("workflows"),
        Some("workflows"),
        "Tuff has no workflow capability",
    ),
    (
        "themes",
        Some("themes"),
        None,
        "Tuff has no theme capability",
    ),
    (
        "monitors",
        Some("monitors"),
        None,
        "Tuff has no monitor capability",
    ),
    (
        "experimental",
        None,
        Some("experimental"),
        "Tuff does not read experimental plugin components",
    ),
    (
        "user-config",
        None,
        Some("userConfig"),
        "Tuff has no per-plugin settings; MCP servers read environment variables",
    ),
    (
        "channels",
        None,
        Some("channels"),
        "Tuff has no channel capability",
    ),
    (
        "dependencies",
        None,
        Some("dependencies"),
        "Tuff does not resolve dependencies between packs",
    ),
];

struct Reader<'a> {
    root: &'a Path,
    name: String,
    version: String,
    manifest: Map<String, Value>,
    /// The marketplace entry's own components, when the plugin also has a
    /// manifest: the entry adds skills to it.
    entry_extra: Option<Map<String, Value>>,
    members: Vec<ImportedMember>,
    unsupported: Vec<PackUnsupported>,
    notes: Vec<String>,
    consumed: BTreeSet<String>,
}

impl<'a> Reader<'a> {
    fn new(bundle: &'a LocalBundle, version_hint: Option<&str>) -> Result<Self> {
        let root = bundle.root.as_path();
        let has_manifest = root.join(PLUGIN_MANIFEST).is_file();
        let entry = bundle.entry.as_ref().and_then(Value::as_object).cloned();
        // Without a plugin.json, a marketplace entry is the plugin's manifest.
        let (manifest, entry_extra) = match (has_manifest, entry) {
            (true, entry) => match read_json(root, PLUGIN_MANIFEST)? {
                Value::Object(manifest) => (manifest, entry),
                _ => {
                    return Err(TuffError::corrupt(format!(
                        "{PLUGIN_MANIFEST} is not a JSON object"
                    )));
                }
            },
            (false, Some(entry)) => (entry, None),
            (false, None) => (Map::new(), None),
        };
        let name = manifest
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| bundle.name.clone())
            .or_else(|| {
                root.file_name()
                    .map(|name| name.to_string_lossy().to_string())
            })
            .ok_or_else(|| TuffError::usage("the plugin has no name"))?;
        if name.is_empty()
            || name
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '/' | '\\' | '@' | ':'))
        {
            return Err(TuffError::corrupt(format!(
                "plugin name '{}' is not a plain name",
                name.escape_debug()
            )));
        }
        let mut notes = Vec::new();
        let version = match (
            manifest.get("version").and_then(Value::as_str),
            version_hint,
        ) {
            (Some(version), _) if !version.trim().is_empty() => version.to_string(),
            (_, Some(hint)) => {
                notes.push(format!(
                    "the plugin declares no version; using {hint}, the version its release tag names"
                ));
                hint.to_string()
            }
            _ => {
                notes.push(
                    "the plugin declares no version; using 0.0.0 for the pack and its members"
                        .to_string(),
                );
                "0.0.0".to_string()
            }
        };
        Ok(Self {
            root,
            name,
            version,
            manifest,
            entry_extra,
            members: Vec::new(),
            unsupported: Vec::new(),
            notes,
            consumed: BTreeSet::new(),
        })
    }

    fn read(mut self) -> Result<ImportedPack> {
        self.read_skills()?;
        self.read_mcp_servers()?;
        self.record_unsupported_components();

        let (files, links) = bundle::read_tree(self.root, Path::new(""))?;
        for link in links {
            self.unsupport(
                "link",
                None,
                Some(&link),
                "a symbolic link, which Tuff does not follow",
            );
        }
        let extra = files
            .into_iter()
            .filter(|file| !self.consumed.contains(&file.path))
            .collect::<Vec<_>>();

        let text = |key: &str| {
            self.manifest
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let author = match self.manifest.get("author") {
            Some(Value::Object(author)) => {
                author
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|name| PackAuthor {
                        name: name.to_string(),
                        email: author
                            .get("email")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        url: author
                            .get("url")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    })
            }
            Some(Value::String(name)) => Some(PackAuthor {
                name: name.clone(),
                email: None,
                url: None,
            }),
            _ => None,
        };
        let keywords = self
            .manifest
            .get("keywords")
            .and_then(Value::as_array)
            .map(|words| {
                words
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|word| !word.trim().is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let description = text("description")
            .filter(|description| !description.trim().is_empty())
            .unwrap_or_else(|| format!("The {} Claude Code plugin.", self.name));
        let manifest = PackManifest {
            schema: tuff_core::pack::PACK_SCHEMA_VERSION,
            name: self.name.clone(),
            version: self.version.clone(),
            description,
            homepage: text("homepage"),
            license: text("license"),
            keywords,
            author,
            build: PackBuild {
                targets: vec!["claude".to_string(), "open-agents".to_string()],
            },
            project: None,
            capabilities: Vec::new(),
            import: Some(PackImport {
                format: FORMAT_ID.to_string(),
                url: None,
                path: None,
                plugin: None,
                commit: None,
                unsupported: self.unsupported,
            }),
            x: BTreeMap::new(),
        };
        if self.members.is_empty() {
            let unsupported = manifest
                .import
                .as_ref()
                .map(|import| import.unsupported.as_slice())
                .unwrap_or_default();
            let has = unsupported
                .iter()
                .map(|item| match &item.name {
                    Some(name) => format!("{} {name}", item.kind),
                    None => item.kind.clone(),
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let has = if has.is_empty() {
                String::new()
            } else {
                format!("; it has {}", has.join(", "))
            };
            return Err(TuffError::unsupported(format!(
                "plugin {} has no skills or MCP servers Tuff can carry{has}",
                self.name
            )));
        }
        Ok(ImportedPack {
            manifest,
            members: self.members,
            extra,
            notes: self.notes,
        })
    }

    fn unsupport(&mut self, kind: &str, name: Option<&str>, path: Option<&str>, reason: &str) {
        self.unsupported.push(PackUnsupported {
            kind: kind.to_string(),
            name: name.map(str::to_string),
            path: path.map(str::to_string),
            reason: reason.to_string(),
        });
    }

    fn member_id_taken(&self, id: &str) -> bool {
        self.members.iter().any(|member| member.id == id)
    }

    // ── skills ────────────────────────────────────────────────────────

    fn read_skills(&mut self) -> Result<()> {
        let mut folders: Vec<PathBuf> = Vec::new();
        let push_skill_folders =
            |root: &Path, relative: PathBuf, folders: &mut Vec<PathBuf>| -> Result<()> {
                let folder = root.join(&relative);
                if folder.join("SKILL.md").is_file() {
                    folders.push(relative);
                    return Ok(());
                }
                if !folder.is_dir() {
                    return Ok(());
                }
                let mut children =
                    std::fs::read_dir(&folder)?.collect::<std::io::Result<Vec<_>>>()?;
                children.sort_by_key(std::fs::DirEntry::file_name);
                for child in children {
                    if child.file_type()?.is_dir() && child.path().join("SKILL.md").is_file() {
                        folders.push(relative.join(child.file_name()));
                    }
                }
                Ok(())
            };
        push_skill_folders(self.root, PathBuf::from("skills"), &mut folders)?;
        let mut declared = false;
        for manifest in [Some(&self.manifest), self.entry_extra.as_ref()]
            .into_iter()
            .flatten()
        {
            for path in paths_of(manifest.get("skills")) {
                declared = true;
                push_skill_folders(self.root, bundle::bundle_path(&path)?, &mut folders)?;
            }
        }
        if folders.is_empty() && !declared && self.root.join("SKILL.md").is_file() {
            folders.push(PathBuf::new());
        }
        let mut seen = BTreeSet::new();
        for folder in folders {
            if seen.insert(folder.clone()) {
                self.read_skill(&folder)?;
            }
        }
        Ok(())
    }

    fn read_skill(&mut self, folder: &Path) -> Result<()> {
        let relative = folder.to_string_lossy().replace('\\', "/");
        let id = if relative.is_empty() {
            self.name.clone()
        } else {
            folder
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default()
        };
        let shown = if relative.is_empty() {
            "SKILL.md".to_string()
        } else {
            relative.clone()
        };
        if manifest::validate_capability_id(&id).is_err() || self.member_id_taken(&id) {
            self.unsupport(
                "skill",
                Some(&id),
                Some(&shown),
                "its name is not a usable capability id, or another member already has it",
            );
            return Ok(());
        }
        let (files, links) = bundle::read_tree(self.root, folder)?;
        for link in &links {
            self.unsupport(
                "link",
                Some(&id),
                Some(link),
                "a symbolic link in a skill, which Tuff does not follow",
            );
        }
        let prefix = if relative.is_empty() {
            String::new()
        } else {
            format!("{relative}/")
        };
        let mut own = Vec::new();
        for file in files {
            // A plugin that is itself one skill: its manifest and MCP config
            // are the plugin's, not the skill's.
            if relative.is_empty()
                && (file.path.starts_with(".claude-plugin/") || file.path == ".mcp.json")
            {
                continue;
            }
            let inner = file
                .path
                .strip_prefix(&prefix)
                .unwrap_or(&file.path)
                .to_string();
            if inner == "tuff.toml" {
                continue;
            }
            self.consumed.insert(file.path.clone());
            own.push(BundleFile {
                path: inner,
                bytes: file.bytes,
            });
        }
        let skill = own
            .iter()
            .find(|file| file.path == "SKILL.md")
            .map(|file| String::from_utf8_lossy(&file.bytes).to_string())
            .unwrap_or_default();
        let description = manifest::frontmatter_description(&skill)
            .filter(|description| !description.trim().is_empty())
            .unwrap_or_else(|| format!("The {id} skill from the {} plugin.", self.name));
        let mut table = toml::Table::new();
        table.insert("id".into(), id.clone().into());
        table.insert("version".into(), self.version.clone().into());
        table.insert("type".into(), "skill".into());
        table.insert("description".into(), description.into());
        table.insert(
            "files".into(),
            toml::Value::Array(own.iter().map(|file| file.path.clone().into()).collect()),
        );
        self.members.push(ImportedMember {
            id,
            capability_type: CapabilityType::Skill,
            manifest: table,
            files: own,
        });
        Ok(())
    }

    // ── MCP servers ───────────────────────────────────────────────────

    fn read_mcp_servers(&mut self) -> Result<()> {
        // `.mcp.json` first, then each declared shape in order; a later
        // server of the same name replaces an earlier one, as Claude Code
        // loads them.
        let mut servers: BTreeMap<String, (Value, String)> = BTreeMap::new();
        if self.root.join(".mcp.json").is_file() {
            let config = read_json(self.root, ".mcp.json")?;
            collect_servers(&config, ".mcp.json", &mut servers);
        }
        let declared = match self.manifest.get("mcpServers") {
            Some(Value::Array(items)) => items.clone(),
            Some(item) => vec![item.clone()],
            None => Vec::new(),
        };
        for item in declared {
            match item {
                Value::String(path) if path.ends_with(".mcpb") || path.contains("://") => {
                    self.unsupport(
                        "mcp-bundle",
                        None,
                        Some(&path),
                        "an MCP bundle, which Tuff does not unpack",
                    );
                }
                Value::String(path) => {
                    let relative = bundle::bundle_path(&path)?
                        .to_string_lossy()
                        .replace('\\', "/");
                    let config = read_json(self.root, &relative)?;
                    collect_servers(&config, &relative, &mut servers);
                }
                Value::Object(_) => collect_servers(&item, "plugin.json mcpServers", &mut servers),
                _ => {}
            }
        }
        for (name, (entry, origin)) in servers {
            if manifest::validate_capability_id(&name).is_err() || self.member_id_taken(&name) {
                self.unsupport(
                    "mcp-server",
                    Some(&name),
                    Some(&origin),
                    "its name is not a usable capability id, or another member already has it",
                );
                continue;
            }
            match self.convert_server(&name, &entry) {
                Ok(table) => self.members.push(ImportedMember {
                    id: name,
                    capability_type: CapabilityType::McpServer,
                    manifest: table,
                    files: Vec::new(),
                }),
                Err(reason) => self.unsupport("mcp-server", Some(&name), Some(&origin), &reason),
            }
        }
        Ok(())
    }

    /// One server as a `tuff.toml`, or the reason it cannot be one.
    fn convert_server(
        &mut self,
        name: &str,
        entry: &Value,
    ) -> std::result::Result<toml::Table, String> {
        let entry = entry.as_object().ok_or("its entry is not a JSON object")?;
        let transport = match entry.get("type").and_then(Value::as_str) {
            None | Some("stdio") => "stdio",
            Some("http" | "streamable-http") => "http",
            Some(other) => {
                return Err(format!(
                    "it uses the '{other}' transport; Tuff writes stdio and HTTP servers"
                ));
            }
        };
        let allowed: &[&str] = if transport == "stdio" {
            &["type", "command", "args", "env"]
        } else {
            &["type", "url", "headers"]
        };
        if let Some(key) = entry.keys().find(|key| !allowed.contains(&key.as_str())) {
            return Err(format!("its '{key}' setting has no tuff.toml equivalent"));
        }
        let mut server = toml::Table::new();
        server.insert("transport".into(), transport.into());
        if transport == "stdio" {
            let command = entry
                .get("command")
                .and_then(Value::as_str)
                .ok_or("it has no command")?;
            let args = match entry.get("args") {
                None => Vec::new(),
                Some(Value::Array(args)) => args
                    .iter()
                    .map(|arg| {
                        arg.as_str()
                            .map(str::to_string)
                            .ok_or("an argument is not a string")
                    })
                    .collect::<std::result::Result<Vec<_>, _>>()?,
                Some(_) => return Err("its args are not a list".into()),
            };
            for text in std::iter::once(command).chain(args.iter().map(String::as_str)) {
                if let Some(reason) = placeholder_problem(text) {
                    return Err(reason);
                }
            }
            server.insert("command".into(), command.into());
            server.insert(
                "args".into(),
                toml::Value::Array(args.into_iter().map(Into::into).collect()),
            );
            let mut env = toml::Table::new();
            for (key, value) in entry
                .get("env")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
            {
                let value = value
                    .as_str()
                    .ok_or_else(|| format!("its {key} is not a string"))?;
                let Some((variable, default)) = whole_reference(value) else {
                    return Err(format!(
                        "it sets {key} to a value written into the config; Tuff records only references to environment variables, so a secret never lands in a manifest"
                    ));
                };
                if variable.starts_with("CLAUDE_") || variable.starts_with("user_config.") {
                    return Err(format!(
                        "its {key} reads ${{{variable}}}, which only Claude Code provides"
                    ));
                }
                if default.is_some() {
                    self.notes.push(format!(
                        "MCP server {name}: the default for ${{{variable}}} is dropped; set {variable} where the agent runs"
                    ));
                }
                let mut reference = toml::Table::new();
                reference.insert("from_env".into(), variable.into());
                env.insert(key.clone(), reference.into());
            }
            server.insert("env".into(), env.into());
        } else {
            let url = entry
                .get("url")
                .and_then(Value::as_str)
                .ok_or("it has no url")?;
            if url.contains("${") {
                return Err(
                    "its url reads an environment variable, which a tuff.toml url cannot".into(),
                );
            }
            server.insert("url".into(), url.into());
            let mut headers = toml::Table::new();
            for (key, value) in entry
                .get("headers")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
            {
                let value = value
                    .as_str()
                    .ok_or_else(|| format!("its {key} header is not a string"))?;
                let (variable, format, default) = embedded_reference(value).ok_or_else(|| {
                    format!("its {key} header is a value written into the config; Tuff records only references to environment variables")
                })?;
                if variable.starts_with("CLAUDE_") || variable.starts_with("user_config.") {
                    return Err(format!(
                        "its {key} header reads ${{{variable}}}, which only Claude Code provides"
                    ));
                }
                if default {
                    self.notes.push(format!(
                        "MCP server {name}: the default for ${{{variable}}} is dropped; set {variable} where the agent runs"
                    ));
                }
                let mut reference = toml::Table::new();
                reference.insert("from_env".into(), variable.into());
                if format != manifest::FORMAT_PLACEHOLDER {
                    reference.insert("format".into(), format.into());
                }
                headers.insert(key.clone(), reference.into());
            }
            if !headers.is_empty() {
                server.insert("headers".into(), headers.into());
            }
        }
        // The same parse `tuff.toml` gets, so a server that passes here
        // cannot fail later when the pack is loaded.
        toml::Value::Table(server.clone())
            .try_into::<McpServerConfig>()
            .map_err(|error| format!("it does not make a valid [server] section: {error}"))?;

        let mut table = toml::Table::new();
        table.insert("id".into(), name.into());
        table.insert("version".into(), self.version.clone().into());
        table.insert("type".into(), "mcp-server".into());
        table.insert(
            "description".into(),
            format!("The {name} MCP server from the {} plugin.", self.name).into(),
        );
        table.insert("server".into(), server.into());
        Ok(table)
    }

    // ── what is left ──────────────────────────────────────────────────

    fn record_unsupported_components(&mut self) {
        for (kind, folder, key, reason) in UNSUPPORTED_COMPONENTS {
            if let Some(folder) = folder
                && self.root.join(folder).exists()
            {
                self.unsupport(kind, None, Some(folder), reason);
            }
            for manifest in [Some(self.manifest.clone()), self.entry_extra.clone()]
                .into_iter()
                .flatten()
            {
                if let Some(key) = key
                    && manifest.contains_key(*key)
                {
                    let place = format!("{PLUGIN_MANIFEST}: {key}");
                    if !self.unsupported.iter().any(|item| {
                        item.kind == *kind && item.path.as_deref() == Some(place.as_str())
                    }) {
                        self.unsupport(kind, None, Some(&place), reason);
                    }
                }
            }
        }
    }
}

/// Paths a manifest key holds as a string or a list of strings.
fn paths_of(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(path)) => vec![path.clone()],
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

/// Servers from a config: `{ "mcpServers": { … } }`, or the map itself.
fn collect_servers(config: &Value, origin: &str, servers: &mut BTreeMap<String, (Value, String)>) {
    let map = config
        .get("mcpServers")
        .and_then(Value::as_object)
        .or_else(|| {
            config
                .as_object()
                .filter(|map| !map.contains_key("mcpServers"))
        });
    for (name, entry) in map.into_iter().flatten() {
        servers.insert(name.clone(), (entry.clone(), origin.to_string()));
    }
}

/// Why a command or argument cannot be carried, if it uses a placeholder.
fn placeholder_problem(text: &str) -> Option<String> {
    if text.contains("${CLAUDE_PLUGIN_ROOT}") || text.contains("${CLAUDE_PLUGIN_DATA}") {
        return Some(
            "it runs a program bundled in the plugin (${CLAUDE_PLUGIN_ROOT}), which Tuff does not install".into(),
        );
    }
    text.contains("${").then(|| {
        "its command or arguments use ${…}, which harnesses other than Claude Code do not expand".into()
    })
}

/// `${VAR}` or `${VAR:-default}` making up the whole value: the variable,
/// and the default when there is one.
fn whole_reference(value: &str) -> Option<(String, Option<String>)> {
    let inner = value.strip_prefix("${")?.strip_suffix('}')?;
    if inner.contains("${") || inner.contains('}') {
        return None;
    }
    let (variable, default) = match inner.split_once(":-") {
        Some((variable, default)) => (variable, Some(default.to_string())),
        None => (inner, None),
    };
    valid_variable(variable).then(|| (variable.to_string(), default))
}

/// Exactly one `${VAR}` or `${VAR:-default}` inside a header value: the
/// variable, the value with `{}` in its place, and whether a default was
/// dropped.
fn embedded_reference(value: &str) -> Option<(String, String, bool)> {
    let start = value.find("${")?;
    let end = start + value[start..].find('}')?;
    let rest = &value[end + 1..];
    if rest.contains("${") || value.contains(manifest::FORMAT_PLACEHOLDER) {
        return None;
    }
    let (variable, default) = whole_reference(&value[start..=end])?;
    let format = format!(
        "{}{}{}",
        &value[..start],
        manifest::FORMAT_PLACEHOLDER,
        rest
    );
    Some((variable, format, default.is_some()))
}

fn valid_variable(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_reference_is_a_variable_and_anything_else_is_a_literal() {
        assert_eq!(
            whole_reference("${GITHUB_TOKEN}"),
            Some(("GITHUB_TOKEN".into(), None))
        );
        assert_eq!(
            whole_reference("${REGION:-us-east-1}"),
            Some(("REGION".into(), Some("us-east-1".into())))
        );
        assert_eq!(whole_reference("ghp_literal"), None);
        assert_eq!(whole_reference("prefix-${TOKEN}"), None);
        assert_eq!(whole_reference("${A}${B}"), None);
    }

    #[test]
    fn a_header_keeps_its_text_around_one_reference() {
        assert_eq!(
            embedded_reference("Bearer ${API_KEY}"),
            Some(("API_KEY".into(), "Bearer {}".into(), false))
        );
        assert_eq!(
            embedded_reference("${KEY}"),
            Some(("KEY".into(), "{}".into(), false))
        );
        assert_eq!(embedded_reference("Bearer abc123"), None);
        assert_eq!(embedded_reference("${A} ${B}"), None);
    }

    #[test]
    fn a_bundled_program_is_named_as_the_reason() {
        assert!(
            placeholder_problem("${CLAUDE_PLUGIN_ROOT}/server.js")
                .unwrap()
                .contains("bundled in the plugin")
        );
        assert!(placeholder_problem("npx").is_none());
        assert!(placeholder_problem("--dir=${HOME}").is_some());
    }
}
