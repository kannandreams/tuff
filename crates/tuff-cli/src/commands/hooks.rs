use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;
use tuff_core::adapter::HookSettingsShape;
use tuff_hooks_spec::{
    BlockingScope, CompatibilityEntry, CompatibilityMatrix, CoverageLevel, EVENT_SPECS,
    HookEventSpec, SPEC_VERSION,
};

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
        rows.extend(matrix_rows(adapter));
        notes.extend(matrix_notes(adapter));
    }

    print_matrix(&rows, &notes);
    Ok(())
}

/// The hook specification this binary implements, as a document.
///
/// This is what `spec/hooks/hooks-spec.json` in the repository is generated
/// from, and what the website's specification page renders. It is built
/// from the same constants the adapters run on (`EVENT_SPECS` and each
/// adapter's matrix), so the published spec cannot say something the code
/// does not do. Field names are part of the published format; see
/// `spec/hooks/hooks-spec.schema.json`.
#[derive(Serialize)]
pub struct HooksSpecDocument {
    pub spec_version: &'static str,
    pub events: &'static [HookEventSpec],
    pub adapters: Vec<AdapterSpec>,
}

/// One harness's declaration: where its hooks live, what shape the
/// settings file takes, and the compatibility matrix for every event.
#[derive(Serialize)]
pub struct AdapterSpec {
    pub adapter: &'static str,
    pub display_name: &'static str,
    pub dir_prefix: &'static str,
    pub hook_settings_path: &'static str,
    pub hook_settings_shape: HookSettingsShape,
    /// The command a conforming implementation registers for a hook with
    /// id `<id>`, relative to the project root.
    pub hook_command: String,
    #[serde(flatten)]
    pub matrix: &'static CompatibilityMatrix,
}

pub fn hooks_spec_document() -> HooksSpecDocument {
    HooksSpecDocument {
        spec_version: SPEC_VERSION,
        events: EVENT_SPECS,
        adapters: AdapterKind::all()
            .into_iter()
            .map(|adapter| AdapterSpec {
                adapter: adapter.id(),
                display_name: adapter.display_name(),
                dir_prefix: adapter.dir_prefix(),
                hook_settings_path: adapter.hook_settings_relpath(),
                hook_settings_shape: adapter.hook_settings_shape(),
                hook_command: format!(
                    "sh {}/hooks/<id>/{}",
                    adapter.dir_prefix(),
                    adapter.hook_filename()
                ),
                matrix: adapter.hook_compatibility(),
            })
            .collect(),
    }
}

/// Print the hook specification this binary implements: the canonical
/// events and every adapter's compatibility matrix, registered or not.
/// `--json` prints the document the repository's `spec/hooks/` files are
/// generated from.
pub fn cmd_hooks_spec(json: bool) -> Result<()> {
    let document = hooks_spec_document();
    if json {
        println!("{}", serde_json::to_string_pretty(&document)?);
        return Ok(());
    }

    println!("Tuff hooks specification {}\n", document.spec_version);
    let event_rows: Vec<Vec<String>> = document
        .events
        .iter()
        .map(|spec| {
            vec![
                spec.canonical_name.to_string(),
                blocking_label(spec.blocking),
                spec.since_spec_version.to_string(),
                spec.payload_schema
                    .fields
                    .iter()
                    .map(|field| field.name)
                    .collect::<Vec<_>>()
                    .join(", "),
            ]
        })
        .collect();
    println!(
        "{}",
        render_table(&["EVENT", "BLOCKING", "SINCE", "PAYLOAD"], &event_rows)
    );

    let mut rows = Vec::new();
    let mut notes = Vec::new();
    for adapter in AdapterKind::all() {
        rows.extend(matrix_rows(adapter));
        notes.extend(matrix_notes(adapter));
    }
    println!();
    print_matrix(&rows, &notes);
    Ok(())
}

fn matrix_rows(adapter: AdapterKind) -> Vec<Vec<String>> {
    adapter
        .hook_compatibility()
        .events
        .iter()
        .map(|entry| {
            vec![
                adapter.id().to_string(),
                entry.event.to_string(),
                entry.native_event.unwrap_or("").to_string(),
                coverage_label(entry.coverage).to_string(),
                entry.scope.join(", "),
                version_label(entry),
            ]
        })
        .collect()
}

fn matrix_notes(adapter: AdapterKind) -> Vec<String> {
    adapter
        .hook_compatibility()
        .events
        .iter()
        .filter_map(|entry| {
            entry
                .caveat
                .map(|caveat| format!("{} / {}: {}", adapter.id(), entry.event, caveat))
        })
        .collect()
}

fn print_matrix(rows: &[Vec<String>], notes: &[String]) {
    println!(
        "{}",
        render_table(
            &[
                "ADAPTER", "EVENT", "NATIVE", "COVERAGE", "SCOPE", "VERSIONS"
            ],
            rows
        )
    );
    if !notes.is_empty() {
        println!("\nNotes:");
        for note in notes {
            println!("- {note}");
        }
    }
}

fn blocking_label(blocking: BlockingScope) -> String {
    match blocking {
        BlockingScope::NotBlocking => "not blocking".to_string(),
        BlockingScope::BlocksAction => "blocks action".to_string(),
        BlockingScope::BlocksContinuation => "blocks continuation".to_string(),
        BlockingScope::Custom(note) => format!("custom: {note}"),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tuff_hooks_spec::HookEvent;

    /// What the published spec promises of a compatibility matrix: every
    /// canonical event appears exactly once, and the matrix targets the
    /// spec version it is published with. This is the conformance check an
    /// external implementation runs against its own matrix; Tuff's four
    /// pass it here so the generated `spec/hooks/hooks-spec.json` can never
    /// carry a matrix that would fail it.
    #[test]
    fn every_adapter_matrix_covers_every_canonical_event_exactly_once() {
        let canonical: Vec<HookEvent> = EVENT_SPECS.iter().map(|spec| spec.event).collect();
        for adapter in hooks_spec_document().adapters {
            assert_eq!(
                adapter.matrix.spec_version, SPEC_VERSION,
                "{}",
                adapter.adapter
            );
            assert_eq!(adapter.matrix.adapter, adapter.adapter);
            let mut listed: Vec<HookEvent> = adapter
                .matrix
                .events
                .iter()
                .map(|entry| entry.event)
                .collect();
            listed.sort();
            let mut expected = canonical.clone();
            expected.sort();
            assert_eq!(listed, expected, "{} matrix", adapter.adapter);
            for entry in adapter.matrix.events {
                assert_eq!(
                    entry.coverage.is_supported(),
                    entry.native_event.is_some(),
                    "{} / {}: a supported row names its native event, an unsupported one does not",
                    adapter.adapter,
                    entry.event
                );
            }
        }
    }

    #[test]
    fn the_event_table_names_each_event_by_its_canonical_name() {
        for spec in EVENT_SPECS {
            assert_eq!(spec.canonical_name, spec.event.as_str());
            assert_eq!(spec.since_spec_version, SPEC_VERSION);
        }
    }
}
