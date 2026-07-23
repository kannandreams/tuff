use std::collections::BTreeSet;
use std::path::Path;

use coral_hooks_spec::{CompatibilityEntry, CoverageLevel};

use crate::adapter::{AdapterKind, AgentAdapter};
use crate::config;
use crate::error::{CoralError, Result};
use crate::lockfile;
use crate::manifest::CapabilityType;

use super::render_table;

pub fn cmd_hooks_matrix(repo_root: &Path) -> Result<()> {
    let adapters = registered_adapters(repo_root)?;
    let mut rows = Vec::new();

    for adapter in adapters {
        for entry in adapter.hook_compatibility().events {
            rows.push(vec![
                adapter.id().to_string(),
                entry.event.to_string(),
                entry.native_event.unwrap_or("").to_string(),
                coverage_label(entry.coverage).to_string(),
                entry.scope.join(", "),
                version_label(entry),
                entry.caveat.unwrap_or("").to_string(),
            ]);
        }
    }

    println!(
        "{}",
        render_table(
            &[
                "ADAPTER", "EVENT", "NATIVE", "COVERAGE", "SCOPE", "VERSIONS", "CAVEAT"
            ],
            &rows
        )
    );
    Ok(())
}

pub fn cmd_hooks_check_portability(repo_root: &Path, hook_id: &str, target: &str) -> Result<()> {
    let target = AdapterKind::from_id(target).ok_or_else(|| {
        CoralError::new(format!(
            "unknown agent '{}'; use 'coral agent list' to see available agents",
            target
        ))
    })?;
    ensure_registered(repo_root, target)?;

    let lock = lockfile::require_lockfile(repo_root)?;
    let entry = lock.capabilities.get(hook_id).ok_or_else(|| {
        CoralError::new(format!(
            "hook capability '{}' is not tracked in coral.lock",
            hook_id
        ))
    })?;
    if entry.capability_type != CapabilityType::Hook {
        return Err(CoralError::new(format!(
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
    for event in events {
        let matrix = target.hook_compatibility();
        let Some(compat) = matrix.find_event(&event) else {
            rows.push(vec![
                event,
                target.id().to_string(),
                "unsupported".to_string(),
                String::new(),
                "target adapter has no compatibility row for this event".to_string(),
            ]);
            continue;
        };
        rows.push(vec![
            event,
            target.id().to_string(),
            coverage_label(compat.coverage).to_string(),
            compat.scope.join(", "),
            compat.caveat.unwrap_or("").to_string(),
        ]);
    }

    println!(
        "{}",
        render_table(&["EVENT", "TARGET", "STATUS", "SCOPE", "CAVEAT"], &rows)
    );
    Ok(())
}

fn registered_adapters(repo_root: &Path) -> Result<Vec<AdapterKind>> {
    let config = config::read_config(repo_root)?;
    let mut adapters = Vec::new();
    for id in config.agents {
        let adapter = AdapterKind::from_id(&id).ok_or_else(|| {
            CoralError::new(format!(
                "unknown registered agent '{}'; use 'coral agent list' to inspect config",
                id
            ))
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
    Err(CoralError::new(format!(
        "agent '{}' is not registered in this project; run 'coral agent add {}' first",
        adapter.id(),
        adapter.id()
    )))
}

fn tracked_hook_events(entry: &lockfile::CapabilityLockEntry) -> BTreeSet<String> {
    entry
        .targets
        .values()
        .flat_map(|target| target.managed_hooks.iter().map(|hook| hook.event.clone()))
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
