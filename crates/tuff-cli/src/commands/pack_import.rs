//! `tuff pack import` (RFC-110 D11): a Claude Code plugin written as a
//! standalone source pack, with what it has that Tuff does not carry
//! recorded and kept.

use std::path::Path;

use tuff_core::bundle;
use tuff_packs::import::{ImportRequest, import};

use crate::error::Result;
use crate::pack::PackUnsupported;

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
    let imported = import(ImportRequest {
        source,
        plugin,
        git_ref,
        path,
    })?;
    bundle::write_source_pack(output, &imported)?;
    for note in &imported.notes {
        eprintln!("note: {note}");
    }
    if json {
        let built = tuff_packs::build::build_for_declared_targets(output)?;
        super::pack::print_build_notes(&built);
        let source = serde_json::json!({ "kind": "folder", "path": output.display().to_string() });
        let document = tuff_packs::inspect::document(&built.artifact, &source)?;
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
