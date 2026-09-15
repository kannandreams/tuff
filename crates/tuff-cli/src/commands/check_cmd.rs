use std::path::Path;

use crate::error::Result;

use super::paint;

pub fn cmd_check(
    repo_root: &Path,
    json: bool,
    ignore_failures: bool,
    global: bool,
    strict: bool,
) -> Result<()> {
    let scope = if global {
        crate::check::CheckScope::Global
    } else {
        crate::check::CheckScope::ProjectAndGlobal
    };
    let outcome = crate::check::run_checks(repo_root, scope)?;

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
        // Rules a policy was installed without, because the agent does not
        // enforce them. Reported on every run so the gap stays visible.
        for gap in &outcome.gaps {
            println!(
                // The type column matches the rows above, which print the
                // capability type unpadded.
                "{} {:<24} policy {:<12} rule {} ({}) is not enforced: {}",
                paint("!", "33"),
                gap.id,
                gap.target,
                gap.rule,
                gap.description,
                gap.reason
            );
        }
    }

    let strict_failure = strict && !outcome.gaps.is_empty();
    if strict_failure && !ignore_failures {
        eprintln!(
            "error: {} policy rule(s) are recorded as not enforced, and --strict fails on them",
            outcome.gaps.len()
        );
    }

    if (!outcome.valid || strict_failure) && !ignore_failures {
        std::process::exit(1);
    }

    Ok(())
}
