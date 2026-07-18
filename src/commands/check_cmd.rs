use std::path::Path;

use crate::error::Result;

use super::paint;

pub fn cmd_check(repo_root: &Path, json: bool, ignore_failures: bool, _global: bool) -> Result<()> {
    let outcome = crate::check::run_checks(repo_root)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&outcome)?);
    } else {
        for r in &outcome.results {
            let mark = if r.status == "ok" {
                paint("✓", "32")
            } else {
                paint("✗", "31")
            };
            let status = if r.status == "ok" {
                paint("ok", "32")
            } else {
                paint(&r.status, "31")
            };
            let extra = if !r.files.is_empty() {
                format!(" ({})", r.files.join(", "))
            } else {
                String::new()
            };
            println!(
                "{mark} {:<24} {:<10} {:<12} {}{}",
                r.id, r.capability_type, r.target, status, extra
            );
        }
    }

    if !outcome.valid && !ignore_failures {
        std::process::exit(1);
    }

    Ok(())
}
