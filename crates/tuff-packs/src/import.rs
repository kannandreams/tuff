//! Reading a Claude Code plugin as a pack (RFC-110 D11 and D12): the
//! import behind `tuff pack import` and behind installing a plugin from
//! git, with what the plugin has that Tuff does not carry recorded.

use std::path::PathBuf;

use tempfile::TempDir;

use tuff_adapter_claude::plugin::ClaudePlugin;
use tuff_core::bundle::{self, BundleLocation, BundleReader, ImportedPack, LocalBundle};

use tuff_core::error::{Result, TuffError};
use tuff_core::lockfile::PackGitSource;
use tuff_core::pack::{PackImport, PackUnsupported};

use crate::adapter::{AdapterKind, AgentAdapter};
use crate::git::{self as pack_git, CheckedOut, CheckoutRequest, GitPackCheckout};

/// Read a plugin and record where it came from on the pack's manifest.
pub fn import_plugin(
    bundle: &LocalBundle,
    version_hint: Option<&str>,
    origin: Origin,
) -> Result<ImportedPack> {
    let mut imported = ClaudePlugin.read(bundle, version_hint)?;
    // A plugin's skills and MCP servers are worth having in every harness
    // that can take all of them, which is the point of reading it as a
    // pack. A harness that cannot write one member is left out, with a note.
    let mut targets = Vec::new();
    for adapter in AdapterKind::all() {
        match first_unwritable(adapter, &imported) {
            None => targets.push(adapter.id().to_string()),
            Some(reason) => imported.notes.push(format!(
                "the pack is not built for {}: {reason}",
                adapter.display_name()
            )),
        }
    }
    imported.manifest.build.targets = targets;
    if let Some(import) = imported.manifest.import.as_mut() {
        import.url = origin.url;
        import.path = origin.path;
        import.plugin = origin.plugin;
        import.commit = origin.commit;
    }
    Ok(imported)
}

/// Why `adapter` cannot write a member of the pack, if it cannot: a kind it
/// does not take, or an MCP server it cannot express.
fn first_unwritable(adapter: AdapterKind, imported: &ImportedPack) -> Option<String> {
    for member in &imported.members {
        if !adapter.supports(member.capability_type) {
            return Some(format!(
                "it does not take {} '{}'",
                member.capability_type, member.id
            ));
        }
        if let Some(server) = member.manifest.get("server").cloned()
            && let Ok(server) = server.try_into::<tuff_core::manifest::McpServerConfig>()
            && let Err(error) = adapter.mcp_server_entry_checked(&server)
        {
            return Some(error.to_string());
        }
    }
    None
}

/// Where an imported plugin was read from.
#[derive(Default)]
pub struct Origin {
    pub url: Option<String>,
    pub path: Option<String>,
    pub plugin: Option<String>,
    pub commit: Option<String>,
}

impl Origin {
    pub fn of_git(source: &PackGitSource) -> Self {
        Self {
            url: Some(source.url.clone()),
            path: (!source.path.is_empty()).then(|| source.path.clone()),
            plugin: source.plugin.clone(),
            commit: Some(source.git_ref.clone()),
        }
    }
}

/// The source pack a git checkout holds.
pub struct SourcePack {
    /// The temporary folder an imported plugin was written to, deleted
    /// when this is dropped. `None` for a Tuff pack, which is read in place.
    pub directory: Option<TempDir>,
    /// The folder holding `tuff-pack.toml`.
    pub root: PathBuf,
    /// What the import had to say, such as harnesses the pack is not built
    /// for.
    pub notes: Vec<String>,
}

/// The source pack a git checkout holds: its folder for a Tuff pack, or a
/// Claude Code plugin imported into a temporary folder.
pub fn source_pack_of(checkout: &GitPackCheckout) -> Result<SourcePack> {
    match &checkout.found {
        CheckedOut::Tuff => Ok(SourcePack {
            directory: None,
            root: checkout.pack_root.clone(),
            notes: Vec::new(),
        }),
        CheckedOut::ClaudePlugin(bundle) => {
            let imported = import_plugin(
                bundle,
                checkout.version_hint.as_deref(),
                Origin::of_git(&checkout.source),
            )?;
            let directory = tempfile::tempdir()?;
            let output = directory.path().join("pack");
            bundle::write_source_pack(&output, &imported)?;
            Ok(SourcePack {
                directory: Some(directory),
                root: output,
                notes: imported.notes,
            })
        }
    }
}

/// What to import: a plugin or marketplace folder, or a git URL.
pub struct ImportRequest<'a> {
    pub source: &'a str,
    /// The marketplace entry to read.
    pub plugin: Option<&'a str>,
    /// A tag, branch, or commit, for a git URL only.
    pub git_ref: Option<&'a str>,
    /// The folder inside the source holding the plugin or marketplace.
    pub path: Option<&'a str>,
}

/// Read a Claude Code plugin, or one plugin of a marketplace, as a pack.
/// A marketplace entry in another repository is checked out from there.
/// Write the result with [`bundle::write_source_pack`].
pub fn import(request: ImportRequest<'_>) -> Result<ImportedPack> {
    let ImportRequest {
        source,
        plugin,
        git_ref,
        path,
    } = request;
    let imported = if tuff_core::git::is_git_url(source) {
        let checkout = pack_git::checkout(CheckoutRequest {
            url: source,
            path,
            git_ref,
            plugin,
            allow_remote: true,
        })?;
        let CheckedOut::ClaudePlugin(bundle) = &checkout.found else {
            return Err(TuffError::usage(format!(
                "{source} holds a Tuff pack already; install it with 'tuff add pack {source}'"
            )));
        };
        let imported = import_plugin(
            bundle,
            checkout.version_hint.as_deref(),
            Origin::of_git(&checkout.source),
        )?;
        drop(checkout);
        imported
    } else {
        if git_ref.is_some() {
            return Err(TuffError::usage(
                "--ref applies to an import from a git address",
            ));
        }
        let mut root = PathBuf::from(source);
        if let Some(path) = path {
            root.push(bundle::bundle_path(path)?);
        }
        if !ClaudePlugin.detect(&root) {
            return Err(TuffError::not_found(format!(
                "no Claude Code plugin or marketplace at {}",
                root.display()
            ))
            .with_hint("a plugin has .claude-plugin/plugin.json; a marketplace has .claude-plugin/marketplace.json"));
        }
        match ClaudePlugin.locate(&root, plugin)? {
            BundleLocation::Local(bundle) => import_plugin(
                &bundle,
                None,
                Origin {
                    url: Some(root.display().to_string()),
                    plugin: bundle.name.clone(),
                    ..Origin::default()
                },
            )?,
            BundleLocation::Remote(remote) => {
                let (guard, bundle, git) = pack_git::checkout_remote(&remote)?;
                let imported = import_plugin(&bundle, None, Origin::of_git(&git))?;
                drop(guard);
                imported
            }
        }
    };
    Ok(imported)
}

/// The unsupported list an artifact's import records, if any.
pub fn unsupported_of(import: Option<&PackImport>) -> &[PackUnsupported] {
    import
        .map(|import| import.unsupported.as_slice())
        .unwrap_or_default()
}
