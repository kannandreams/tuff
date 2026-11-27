//! A set of packs written as a Claude Code marketplace (RFC-110 D10): one
//! plugin per pack under `plugins/<name>/`, listed in
//! `.claude-plugin/marketplace.json`. The CLI's `tuff pack marketplace build`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use tuff_adapter_claude::plugin::FORMAT_ID;

use crate::build::{Built, build_as};

use tuff_core::error::{Result, TuffError};
use tuff_core::pack::{self, PackArtifact};

/// What to write.
pub struct MarketplaceBuildOptions<'a> {
    pub sources: &'a [PathBuf],
    pub output: &'a Path,
    pub name: Option<&'a str>,
    pub owner_name: &'a str,
    pub owner_url: Option<&'a str>,
    pub skip_unsupported: bool,
}

/// The marketplace that was written.
pub struct BuiltMarketplace {
    pub name: String,
    /// Each plugin's name and version, in the order they are listed.
    pub plugins: Vec<(String, String)>,
    /// What the builds had to say, such as members a plugin left out.
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
}

/// Write `sources` (source pack folders, `.tuffpack` artifacts, or folders
/// holding them) as a marketplace at `output`, which must be missing or
/// empty. The marketplace is staged next to `output` and moved into place
/// once complete.
pub fn build_marketplace(options: MarketplaceBuildOptions<'_>) -> Result<BuiltMarketplace> {
    let MarketplaceBuildOptions {
        sources,
        output,
        name,
        owner_name,
        owner_url,
        skip_unsupported,
    } = options;
    if output.exists() && (!output.is_dir() || fs::read_dir(output)?.next().is_some()) {
        return Err(TuffError::refused(format!(
            "marketplace output must be missing or empty: {}",
            output.display()
        )));
    }
    let name = match name {
        Some(name) => name.to_string(),
        None => output
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .ok_or_else(|| TuffError::usage("name the marketplace with --name"))?,
    };
    validate_marketplace_name(&name)?;
    if owner_name.trim().is_empty() {
        return Err(TuffError::usage("--owner-name must not be empty"));
    }

    let mut inputs = Vec::new();
    for source in sources {
        collect_inputs(source, &mut inputs)?;
    }
    if inputs.is_empty() {
        return Err(TuffError::not_found(
            "no source packs or .tuffpack artifacts found in the given paths",
        ));
    }

    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix("tuff-marketplace-")
        .tempdir_in(parent)?;
    let mut names = BTreeSet::new();
    let mut entries = Vec::new();
    let mut plugins = Vec::new();
    let mut notes = Vec::new();
    let mut warnings = Vec::new();
    for input in &inputs {
        let plugin = plugin_artifact(input, skip_unsupported)?;
        let artifact = match &plugin {
            PluginArtifact::Built(built) => {
                notes.extend(built.notes.iter().cloned());
                warnings.extend(built.warnings.iter().cloned());
                &built.artifact
            }
            PluginArtifact::Read(artifact) => artifact,
        };
        let plugin = plugin_manifest(artifact)?;
        let plugin_name = plugin
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| TuffError::corrupt("the plugin manifest has no name"))?
            .to_string();
        if !names.insert(plugin_name.clone()) {
            return Err(TuffError::usage(format!(
                "two packs are both written as plugin '{plugin_name}'"
            ))
            .with_hint("plugins are named after the last part of the pack name"));
        }
        pack::extract_prefix(
            artifact,
            &format!("targets/{FORMAT_ID}"),
            &staging.path().join("plugins").join(&plugin_name),
        )?;
        let mut entry = serde_json::Map::new();
        entry.insert("name".into(), plugin_name.clone().into());
        entry.insert("source".into(), format!("./plugins/{plugin_name}").into());
        entry.insert(
            "description".into(),
            artifact.metadata.description.clone().into(),
        );
        entry.insert("version".into(), artifact.metadata.version.clone().into());
        entries.push(serde_json::Value::Object(entry));
        plugins.push((plugin_name, artifact.metadata.version.clone()));
    }

    let mut owner = serde_json::Map::new();
    owner.insert("name".into(), owner_name.into());
    if let Some(url) = owner_url {
        owner.insert("url".into(), url.into());
    }
    let marketplace = serde_json::json!({
        "name": name,
        "owner": owner,
        "plugins": entries,
    });
    let manifest = staging.path().join(".claude-plugin/marketplace.json");
    fs::create_dir_all(manifest.parent().expect("has a parent"))?;
    let mut text = serde_json::to_string_pretty(&marketplace)?;
    text.push('\n');
    fs::write(manifest, text)?;
    if output.exists() {
        fs::remove_dir(output)?;
    }
    fs::rename(staging.keep(), output)?;
    Ok(BuiltMarketplace {
        name,
        plugins,
        notes,
        warnings,
    })
}

/// A pack the marketplace is built from.
enum Input {
    Source(PathBuf),
    Artifact(PathBuf),
}

fn collect_inputs(source: &Path, inputs: &mut Vec<Input>) -> Result<()> {
    if source.is_file() {
        if source
            .file_name()
            .is_some_and(|name| name == pack::PACK_MANIFEST_FILE)
        {
            inputs.push(Input::Source(
                source.parent().unwrap_or(source).to_path_buf(),
            ));
        } else {
            inputs.push(Input::Artifact(source.to_path_buf()));
        }
        return Ok(());
    }
    if source.join(pack::PACK_MANIFEST_FILE).is_file() {
        inputs.push(Input::Source(source.to_path_buf()));
        return Ok(());
    }
    if !source.is_dir() {
        return Err(TuffError::not_found(format!(
            "{} does not exist",
            source.display()
        )));
    }
    let mut children = fs::read_dir(source)?.collect::<std::io::Result<Vec<_>>>()?;
    children.sort_by_key(fs::DirEntry::file_name);
    for child in children {
        let path = child.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "tuffpack")
            && path.is_file()
        {
            inputs.push(Input::Artifact(path));
        } else if path.join(pack::PACK_MANIFEST_FILE).is_file() {
            inputs.push(Input::Source(path));
        }
    }
    Ok(())
}

/// An artifact holding the pack's Claude Code plugin: built from a source
/// pack, or read from an artifact that has the target.
enum PluginArtifact {
    Built(Built),
    Read(PackArtifact),
}

fn plugin_artifact(input: &Input, skip_unsupported: bool) -> Result<PluginArtifact> {
    match input {
        Input::Source(folder) => Ok(PluginArtifact::Built(build_as(
            folder,
            &[FORMAT_ID.to_string()],
            skip_unsupported,
        )?)),
        Input::Artifact(path) => {
            let artifact = pack::read_artifact(path)?;
            if !artifact
                .metadata
                .targets
                .iter()
                .any(|target| target.id == FORMAT_ID)
            {
                return Err(TuffError::usage(format!(
                    "{} has no {FORMAT_ID} target",
                    path.display()
                ))
                .with_hint(format!(
                    "add {FORMAT_ID} to its [build] targets and build it again, or pass the source pack"
                )));
            }
            Ok(PluginArtifact::Read(artifact))
        }
    }
}

fn plugin_manifest(artifact: &PackArtifact) -> Result<serde_json::Value> {
    let path = format!("targets/{FORMAT_ID}/.claude-plugin/plugin.json");
    let content = artifact
        .contents
        .iter()
        .find(|content| content.path == path)
        .ok_or_else(|| TuffError::corrupt(format!("the artifact has no {path}")))?;
    Ok(serde_json::from_slice(&content.bytes)?)
}

/// Claude Code's rule for a marketplace name: letters, digits, `.`, `_`,
/// and `-`, starting with a letter or digit, and no `..`.
fn validate_marketplace_name(name: &str) -> Result<()> {
    let valid = name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && !name.contains("..");
    if !valid {
        return Err(TuffError::usage(format!(
            "marketplace name '{name}' must be letters, digits, '.', '_', and '-', starting with a letter or digit"
        ))
        .with_hint("choose one with --name"));
    }
    Ok(())
}
