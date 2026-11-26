//! `tuff pack import` and the on-the-fly imports behind `tuff add pack`
//! (RFC-110 D11 and D12): a Claude Code plugin read as a standalone source
//! pack, with what it has that Tuff does not carry recorded and kept.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

use tuff_adapter_claude::plugin::ClaudePlugin;
use tuff_core::bundle::{self, BundleLocation, BundleReader, ImportedPack, LocalBundle};

use crate::adapter::AgentAdapter;
use crate::error::{Result, TuffError};
use crate::lockfile::PackGitSource;
use crate::pack::{PackImport, PackUnsupported};

use super::pack_git::{self, CheckedOut, CheckoutRequest, GitPackCheckout};

/// Read a plugin and record where it came from on the pack's manifest.
pub(crate) fn import_plugin(
    bundle: &LocalBundle,
    version_hint: Option<&str>,
    origin: Origin,
) -> Result<ImportedPack> {
    let mut imported = ClaudePlugin.read(bundle, version_hint)?;
    // A plugin's skills and MCP servers are worth having in every harness
    // that can take all of them, which is the point of reading it as a
    // pack. A harness that cannot write one member is left out, with a note.
    let mut targets = Vec::new();
    for adapter in crate::adapter::AdapterKind::all() {
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
fn first_unwritable(
    adapter: crate::adapter::AdapterKind,
    imported: &ImportedPack,
) -> Option<String> {
    for member in &imported.members {
        if !adapter.supports(member.capability_type) {
            return Some(format!(
                "it does not take {} '{}'",
                member.capability_type, member.id
            ));
        }
        if let Some(server) = member.manifest.get("server").cloned()
            && let Ok(server) = server.try_into::<crate::manifest::McpServerConfig>()
            && let Err(error) = adapter.mcp_server_entry_checked(&server)
        {
            return Some(error.to_string());
        }
    }
    None
}

/// Where an imported plugin was read from.
#[derive(Default)]
pub(crate) struct Origin {
    pub(crate) url: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) plugin: Option<String>,
    pub(crate) commit: Option<String>,
}

impl Origin {
    fn of_git(source: &PackGitSource) -> Self {
        Self {
            url: Some(source.url.clone()),
            path: (!source.path.is_empty()).then(|| source.path.clone()),
            plugin: source.plugin.clone(),
            commit: Some(source.git_ref.clone()),
        }
    }
}

/// The source pack a git checkout holds: its folder for a Tuff pack, or a
/// Claude Code plugin imported into a temporary folder. The notes the import
/// produced are printed to standard error.
pub(crate) fn source_pack_of(checkout: &GitPackCheckout) -> Result<(Option<TempDir>, PathBuf)> {
    match &checkout.found {
        CheckedOut::Tuff => Ok((None, checkout.pack_root.clone())),
        CheckedOut::ClaudePlugin(bundle) => {
            let imported = import_plugin(
                bundle,
                checkout.version_hint.as_deref(),
                Origin::of_git(&checkout.source),
            )?;
            for note in &imported.notes {
                eprintln!("note: {note}");
            }
            let directory = tempfile::tempdir()?;
            let output = directory.path().join("pack");
            bundle::write_source_pack(&output, &imported)?;
            Ok((Some(directory), output))
        }
    }
}

/// What `tuff pack import` was asked to read.
pub struct PackImportOptions<'a> {
    /// A plugin or marketplace folder, or a git URL.
    pub source: &'a str,
    pub plugin: Option<&'a str>,
    pub git_ref: Option<&'a str>,
    pub path: Option<&'a str>,
    pub output: &'a Path,
    pub json: bool,
}

pub fn cmd_pack_import(options: PackImportOptions<'_>) -> Result<()> {
    let PackImportOptions {
        source,
        plugin,
        git_ref,
        path,
        output,
        json,
    } = options;
    let mut _guards = Vec::new();
    let imported = if crate::git::is_git_url(source) {
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
        _guards.push(checkout);
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
    bundle::write_source_pack(output, &imported)?;
    for note in &imported.notes {
        eprintln!("note: {note}");
    }
    if json {
        let (_built, artifact) = super::pack::build_pack_for_declared_targets(output)?;
        let source = serde_json::json!({ "kind": "folder", "path": output.display().to_string() });
        let document = super::pack_inspect::document(&artifact, &source)?;
        println!("{}", serde_json::to_string_pretty(&document)?);
        return Ok(());
    }
    println!(
        "imported {} {} -> {}",
        imported.manifest.name,
        imported.manifest.version,
        output.display()
    );
    for member in &imported.members {
        println!("  - {} ({})", member.id, member.capability_type);
    }
    print_unsupported(imported.unsupported());
    if !imported.extra.is_empty() {
        println!(
            "kept {} file(s) Tuff does not install under {}",
            imported.extra.len(),
            output.join("extra").display()
        );
    }
    println!("next: tuff pack check {}", output.display());
    Ok(())
}

/// The "not carried" list printed after an import or an install.
pub(crate) fn print_unsupported(unsupported: &[PackUnsupported]) {
    if unsupported.is_empty() {
        return;
    }
    println!("not carried:");
    for item in unsupported {
        let mut what = item.kind.clone();
        if let Some(name) = &item.name {
            what.push(' ');
            what.push_str(name);
        }
        if let Some(path) = &item.path {
            what.push_str(&format!(" ({path})"));
        }
        println!("  - {what}: {}", item.reason);
    }
}

/// The unsupported list an artifact's import records, if any.
pub(crate) fn unsupported_of(import: Option<&PackImport>) -> &[PackUnsupported] {
    import
        .map(|import| import.unsupported.as_slice())
        .unwrap_or_default()
}
