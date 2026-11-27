//! `tuff pack marketplace build` (RFC-110 D10), printing what
//! [`tuff_packs::marketplace::build_marketplace`] wrote.

use crate::error::Result;

pub use tuff_packs::marketplace::MarketplaceBuildOptions;

pub fn cmd_pack_marketplace_build(options: MarketplaceBuildOptions<'_>) -> Result<()> {
    let output = options.output.to_path_buf();
    let built = tuff_packs::marketplace::build_marketplace(options)?;
    for note in &built.notes {
        eprintln!("note: {note}");
    }
    for warning in &built.warnings {
        eprintln!("warning: {warning}");
    }
    for (name, version) in &built.plugins {
        println!("  - {name} {version}");
    }
    println!(
        "built marketplace {} with {} plugin(s) -> {}",
        built.name,
        built.plugins.len(),
        output.display()
    );
    Ok(())
}
