use std::collections::BTreeSet;
use std::path::Path;

use tuff_hooks_spec::{CompatibilityEntry, CoverageLevel};

use crate::adapter::{AdapterKind, AgentAdapter};
use crate::config;
use crate::error::{Result, TuffError};
use crate::lockfile;
use crate::manifest::CapabilityType;

use super::render_table;

pub fn cmd_hooks_matrix(repo_root: &Path) -> Result<()> {
    let adapters = registered_adapters(repo_root)?;
    let mut rows = Vec::new();
    let mut notes = Vec::new();

    for adapter in adapters {
        for entry in adapter.hook_compatibility().events {
            if let Some(caveat) = entry.caveat {
                notes.push(format!("{} / {}: {}", adapter.id(), entry.event, caveat));
            }
            rows.push(vec![
                adapter.id().to_string(),
                entry.event.to_string(),
                entry.native_event.unwrap_or("").to_string(),
                coverage_label(entry.coverage).to_string(),
                entry.scope.join(", "),
                version_label(entry),
            ]);
        }
    }

    println!(
        "{}",
        render_table(
            &[
                "ADAPTER", "EVENT", "NATIVE", "COVERAGE", "SCOPE", "VERSIONS"
            ],
            &rows
        )
    );
    if !notes.is_empty() {
        println!("\nNotes:");
        for note in notes {
            println!("- {note}");
        }
    }
    Ok(())
}

pub fn cmd_hooks_check_portability(repo_root: &Path, hook_id: &str, target: &str) -> Result<()> {
    let target = AdapterKind::from_id(target).ok_or_else(|| {
        TuffError::usage(format!("unknown agent '{}'", target,))
            .with_hint("run 'tuff agent list' to see available agents")
    })?;
    ensure_registered(repo_root, target)?;

    let lock = lockfile::require_lockfile(repo_root)?;
    let entry = lock.capabilities.get(hook_id).ok_or_else(|| {
        TuffError::not_found(format!(
            "hook capability '{}' is not tracked in tuff.lock",
            hook_id
        ))
    })?;
    if entry.capability_type != CapabilityType::Hook {
        return Err(TuffError::usage(format!(
            "'{hook_id}' is not a hook capability"
        )));
    }

    if entry.description == "Added from native hook fragment." {
        println!(
            "note: '{}' was added from a native hook fragment; portability is inferred from tracked native events and is not guaranteed",
            hook_id
        );
    }

    let events = tracked_hook_events(entry);
    if events.is_empty() {
        println!(
            "hook '{}' has no tracked native hook registrations; portability cannot be checked",
            hook_id
        );
        return Ok(());
    }

    let mut rows = Vec::new();
    for tracked in events {
        let matrix = target.hook_compatibility();
        let lookup_event = tracked
            .canonical_event
            .as_deref()
            .unwrap_or(&tracked.native_event);
        let display_event = tracked
            .canonical_event
            .as_deref()
            .unwrap_or(&tracked.native_event)
            .to_string();
        let Some(compat) = matrix.find_event(lookup_event) else {
            rows.push(vec![
                display_event,
                tracked.native_event,
                String::new(),
                target.id().to_string(),
                "unsupported".to_string(),
                String::new(),
                "target adapter has no compatibility row for this event".to_string(),
            ]);
            continue;
        };
        let caveat = match tracked.canonical_event {
            Some(_) => compat.caveat.unwrap_or("").to_string(),
            None => {
                let native_note =
                    "legacy/native event; portability is inferred from its native name";
                match compat.caveat {
                    Some(caveat) => format!("{native_note}; {caveat}"),
                    None => native_note.to_string(),
                }
            }
        };
        rows.push(vec![
            display_event,
            tracked.native_event,
            compat.native_event.unwrap_or("").to_string(),
            target.id().to_string(),
            coverage_label(compat.coverage).to_string(),
            compat.scope.join(", "),
            caveat,
        ]);
    }

    println!(
        "{}",
        render_table(
            &[
                "EVENT",
                "SOURCE NATIVE",
                "TARGET NATIVE",
                "TARGET",
                "STATUS",
                "SCOPE",
                "CAVEAT",
            ],
            &rows,
        )
    );
    Ok(())
}

pub(crate) fn registered_adapters(repo_root: &Path) -> Result<Vec<AdapterKind>> {
    let config = config::read_config(repo_root)?;
    let mut adapters = Vec::new();
    for id in config.agents {
        let adapter = AdapterKind::from_id(&id).ok_or_else(|| {
            TuffError::usage(format!("unknown registered agent '{}'", id))
                .with_hint("run 'tuff agent list' to inspect config")
        })?;
        if !adapters.contains(&adapter) {
            adapters.push(adapter);
        }
    }
    Ok(adapters)
}

fn ensure_registered(repo_root: &Path, adapter: AdapterKind) -> Result<()> {
    let registered = registered_adapters(repo_root)?;
    if registered.contains(&adapter) {
        return Ok(());
    }
    Err(TuffError::usage(format!(
        "agent '{}' is not registered in this project",
        adapter.id()
    ))
    .with_hint(format!("run 'tuff agent add {}' first", adapter.id())))
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct TrackedHookEvent {
    native_event: String,
    canonical_event: Option<String>,
}

fn tracked_hook_events(entry: &lockfile::CapabilityLockEntry) -> BTreeSet<TrackedHookEvent> {
    entry
        .targets
        .values()
        .flat_map(|target| {
            target.managed_hooks.iter().map(|hook| TrackedHookEvent {
                native_event: hook.event.clone(),
                canonical_event: hook.canonical_event.clone(),
            })
        })
        .collect()
}

fn coverage_label(coverage: CoverageLevel) -> &'static str {
    match coverage {
        CoverageLevel::Full => "full",
        CoverageLevel::Partial => "partial",
        CoverageLevel::Unsupported => "unsupported",
    }
}

fn version_label(entry: &CompatibilityEntry) -> String {
    match (entry.since_harness_version, entry.until_harness_version) {
        (Some(since), Some(until)) => format!("{since}..{until}"),
        (Some(since), None) => format!("since {since}"),
        (None, Some(until)) => format!("until {until}"),
        (None, None) => String::new(),
    }
}
