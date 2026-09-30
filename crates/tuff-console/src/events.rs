//! What a report says about a project, flattened, and the audit events that
//! follow from comparing two of them (RFC-108 D7).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

/// Names of the events, as stored and as the API returns them.
pub mod kind {
    pub const PROJECT_FIRST_SEEN: &str = "project_first_seen";
    pub const CAPABILITY_ADDED: &str = "capability_added";
    pub const CAPABILITY_REMOVED: &str = "capability_removed";
    pub const VERSION_CHANGED: &str = "version_changed";
    pub const TARGET_ADDED: &str = "target_added";
    pub const TARGET_REMOVED: &str = "target_removed";
    pub const DRIFT_DETECTED: &str = "drift_detected";
    pub const DRIFT_CLEARED: &str = "drift_cleared";
    pub const POLICY_GAP_ADDED: &str = "policy_gap_added";
    pub const POLICY_GAP_CLOSED: &str = "policy_gap_closed";

    /// Every kind, in the order the UI lists them.
    pub const ALL: &[&str] = &[
        PROJECT_FIRST_SEEN,
        CAPABILITY_ADDED,
        CAPABILITY_REMOVED,
        VERSION_CHANGED,
        TARGET_ADDED,
        TARGET_REMOVED,
        DRIFT_DETECTED,
        DRIFT_CLEARED,
        POLICY_GAP_ADDED,
        POLICY_GAP_CLOSED,
    ];
}

/// One capability installed for one target: a row of the project's lockfile
/// with its check status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryRow {
    pub capability_type: String,
    pub capability_id: String,
    pub target: String,
    pub version: String,
    /// Where it came from, as one line: `git:<url>@<ref>`, `local:<path>`,
    /// `catalog:<id>@<version>`, or `pack:<name>@<version>`.
    pub source: String,
    /// The `tuff check` status of this row, `ok` when the file matches.
    pub status: String,
}

/// A policy rule recorded as not enforced for a target.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Gap {
    pub policy: String,
    pub target: String,
    pub rule: usize,
    pub description: String,
    pub reason: String,
}

/// A report reduced to what the inventory and the audit trail need.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub rows: Vec<InventoryRow>,
    pub gaps: Vec<Gap>,
}

/// One change between two consecutive reports of a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub kind: &'static str,
    pub capability_type: Option<String>,
    pub capability_id: Option<String>,
    pub target: Option<String>,
    pub detail: Option<String>,
}

impl Event {
    fn capability(
        kind: &'static str,
        capability_type: &str,
        capability_id: &str,
        target: Option<String>,
        detail: Option<String>,
    ) -> Self {
        Self {
            kind,
            capability_type: Some(capability_type.to_string()),
            capability_id: Some(capability_id.to_string()),
            target,
            detail,
        }
    }
}

/// A status that does not count as drift. A row the check did not cover is
/// `unknown` and is treated as fine, since nothing says it changed.
fn is_ok(status: &str) -> bool {
    status == "ok" || status == "unknown"
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn source_label(source: &Value) -> String {
    let field = |key: &str| text(source, key);
    match source.get("kind").and_then(Value::as_str) {
        Some("git") => format!("git:{}@{}", field("url"), field("ref")),
        Some("local") => format!("local:{}", field("path")),
        Some("catalog") => format!("catalog:{}@{}", field("id"), field("version")),
        Some("pack") => format!("pack:{}@{}", field("name"), field("version")),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

impl Snapshot {
    /// Read the lockfile rows and check results of a report document. A
    /// report that does not have the expected shape yields an empty
    /// snapshot, so a malformed one never blocks ingest.
    pub fn from_report(report: &Value) -> Self {
        let statuses: BTreeMap<(String, String, String), String> = report
            .pointer("/check/results")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|result| {
                (
                    (
                        text(result, "type"),
                        text(result, "id"),
                        text(result, "target"),
                    ),
                    text(result, "status"),
                )
            })
            .collect();

        let mut rows = Vec::new();
        let mut gaps = Vec::new();
        for entry in report
            .pointer("/lockfile/capabilities")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let capability_type = text(entry, "type");
            let capability_id = text(entry, "name");
            let target = text(entry, "target");
            if capability_id.is_empty() {
                continue;
            }
            let status = statuses
                .get(&(
                    capability_type.clone(),
                    capability_id.clone(),
                    target.clone(),
                ))
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            for rule in entry
                .get("unenforced_rules")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                gaps.push(Gap {
                    policy: capability_id.clone(),
                    target: target.clone(),
                    rule: rule.get("rule").and_then(Value::as_u64).unwrap_or_default() as usize,
                    description: text(rule, "description"),
                    reason: text(rule, "reason"),
                });
            }
            rows.push(InventoryRow {
                capability_type,
                capability_id,
                target,
                version: text(entry, "version"),
                source: entry.get("source").map(source_label).unwrap_or_default(),
                status,
            });
        }
        rows.sort_by(|a, b| {
            (&a.capability_type, &a.capability_id, &a.target).cmp(&(
                &b.capability_type,
                &b.capability_id,
                &b.target,
            ))
        });
        gaps.sort();
        Self { rows, gaps }
    }

    fn by_capability(&self) -> BTreeMap<(&str, &str), Vec<&InventoryRow>> {
        let mut map: BTreeMap<(&str, &str), Vec<&InventoryRow>> = BTreeMap::new();
        for row in &self.rows {
            map.entry((row.capability_type.as_str(), row.capability_id.as_str()))
                .or_default()
                .push(row);
        }
        map
    }
}

fn join_targets<'a>(targets: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let joined = targets.into_iter().collect::<Vec<_>>().join(", ");
    (!joined.is_empty()).then_some(joined)
}

/// The events that turn `previous` into `current`. With no previous report,
/// everything in `current` is new, and the project itself is first seen.
/// Events come in a fixed order, so equal inputs give equal output.
pub fn diff(previous: Option<&Snapshot>, current: &Snapshot) -> Vec<Event> {
    let mut events = Vec::new();
    if previous.is_none() {
        events.push(Event {
            kind: kind::PROJECT_FIRST_SEEN,
            capability_type: None,
            capability_id: None,
            target: None,
            detail: None,
        });
    }
    let empty = Snapshot::default();
    let previous = previous.unwrap_or(&empty);
    let before = previous.by_capability();
    let after = current.by_capability();

    for (&(capability_type, capability_id), rows) in &after {
        let Some(old_rows) = before.get(&(capability_type, capability_id)) else {
            events.push(Event::capability(
                kind::CAPABILITY_ADDED,
                capability_type,
                capability_id,
                join_targets(rows.iter().map(|row| row.target.as_str())),
                rows.first().map(|row| row.version.clone()),
            ));
            continue;
        };
        let old: BTreeMap<&str, &InventoryRow> = old_rows
            .iter()
            .map(|row| (row.target.as_str(), *row))
            .collect();
        let new: BTreeMap<&str, &InventoryRow> =
            rows.iter().map(|row| (row.target.as_str(), *row)).collect();

        for target in new.keys().filter(|target| !old.contains_key(*target)) {
            events.push(Event::capability(
                kind::TARGET_ADDED,
                capability_type,
                capability_id,
                Some((*target).to_string()),
                None,
            ));
        }
        for target in old.keys().filter(|target| !new.contains_key(*target)) {
            events.push(Event::capability(
                kind::TARGET_REMOVED,
                capability_type,
                capability_id,
                Some((*target).to_string()),
                None,
            ));
        }

        // One event per distinct change, so a capability updated for three
        // harnesses is one entry that names them.
        let mut changes: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        for (target, row) in &new {
            let Some(old_row) = old.get(target) else {
                continue;
            };
            if old_row.version != row.version {
                changes
                    .entry(format!("{} to {}", old_row.version, row.version))
                    .or_default()
                    .push(target);
            } else if old_row.source != row.source {
                changes
                    .entry(format!("source {} to {}", old_row.source, row.source))
                    .or_default()
                    .push(target);
            }
        }
        for (detail, targets) in changes {
            events.push(Event::capability(
                kind::VERSION_CHANGED,
                capability_type,
                capability_id,
                join_targets(targets),
                Some(detail),
            ));
        }

        for (target, row) in &new {
            let Some(old_row) = old.get(target) else {
                continue;
            };
            let (was_ok, is_now_ok) = (is_ok(&old_row.status), is_ok(&row.status));
            if was_ok && !is_now_ok {
                events.push(Event::capability(
                    kind::DRIFT_DETECTED,
                    capability_type,
                    capability_id,
                    Some((*target).to_string()),
                    Some(row.status.clone()),
                ));
            } else if !was_ok && is_now_ok {
                events.push(Event::capability(
                    kind::DRIFT_CLEARED,
                    capability_type,
                    capability_id,
                    Some((*target).to_string()),
                    None,
                ));
            }
        }
    }

    for (&(capability_type, capability_id), old_rows) in &before {
        if !after.contains_key(&(capability_type, capability_id)) {
            events.push(Event::capability(
                kind::CAPABILITY_REMOVED,
                capability_type,
                capability_id,
                join_targets(old_rows.iter().map(|row| row.target.as_str())),
                old_rows.first().map(|row| row.version.clone()),
            ));
        }
    }

    let old_gaps: BTreeSet<&Gap> = previous.gaps.iter().collect();
    let new_gaps: BTreeSet<&Gap> = current.gaps.iter().collect();
    for (set, other, kind) in [
        (&new_gaps, &old_gaps, kind::POLICY_GAP_ADDED),
        (&old_gaps, &new_gaps, kind::POLICY_GAP_CLOSED),
    ] {
        for gap in set.difference(other) {
            events.push(Event::capability(
                kind,
                "policy",
                &gap.policy,
                Some(gap.target.clone()),
                Some(format!("rule {}: {}", gap.rule, gap.description)),
            ));
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn report(rows: Value, statuses: Value) -> Value {
        json!({
            "lockfile": { "version": 3, "capabilities": rows },
            "check": { "valid": true, "results": statuses },
        })
    }

    fn skill(name: &str, target: &str, version: &str) -> Value {
        json!({
            "name": name, "type": "skill", "target": target, "version": version,
            "source": { "kind": "git", "url": "https://example.test/x", "ref": "abc" },
        })
    }

    fn status(name: &str, target: &str, status: &str) -> Value {
        json!({ "id": name, "type": "skill", "target": target, "status": status })
    }

    fn kinds(events: &[Event]) -> Vec<&'static str> {
        events.iter().map(|event| event.kind).collect()
    }

    #[test]
    fn a_report_is_flattened_to_one_row_per_capability_and_target() {
        let snapshot = Snapshot::from_report(&report(
            json!([
                skill("lint", "claude", "1.0.0"),
                skill("lint", "codex", "1.0.0")
            ]),
            json!([status("lint", "claude", "modified")]),
        ));
        assert_eq!(snapshot.rows.len(), 2);
        assert_eq!(snapshot.rows[0].target, "claude");
        assert_eq!(snapshot.rows[0].status, "modified");
        assert_eq!(snapshot.rows[1].status, "unknown");
        assert_eq!(snapshot.rows[0].source, "git:https://example.test/x@abc");
    }

    #[test]
    fn a_malformed_report_is_an_empty_snapshot() {
        assert_eq!(
            Snapshot::from_report(&json!({ "lockfile": 3 })),
            Snapshot::default()
        );
    }

    #[test]
    fn the_first_report_names_the_project_and_what_it_carries() {
        let current = Snapshot::from_report(&report(
            json!([skill("lint", "claude", "1.0.0")]),
            json!([]),
        ));
        let events = diff(None, &current);
        assert_eq!(
            kinds(&events),
            [kind::PROJECT_FIRST_SEEN, kind::CAPABILITY_ADDED]
        );
        assert_eq!(events[1].capability_id.as_deref(), Some("lint"));
    }

    #[test]
    fn equal_snapshots_have_no_events() {
        let snapshot = Snapshot::from_report(&report(
            json!([skill("lint", "claude", "1.0.0")]),
            json!([status("lint", "claude", "ok")]),
        ));
        assert!(diff(Some(&snapshot), &snapshot).is_empty());
    }

    #[test]
    fn changes_between_two_reports_are_named() {
        let previous = Snapshot::from_report(&report(
            json!([
                skill("lint", "claude", "1.0.0"),
                skill("lint", "codex", "1.0.0"),
                skill("gone", "claude", "1.0.0"),
                skill("drifty", "claude", "1.0.0"),
                skill("healed", "claude", "1.0.0"),
            ]),
            json!([
                status("drifty", "claude", "ok"),
                status("healed", "claude", "modified"),
            ]),
        ));
        let mut retargeted = skill("lint", "cursor", "1.1.0");
        retargeted["source"]["ref"] = json!("def");
        let current = Snapshot::from_report(&report(
            json!([
                skill("lint", "claude", "1.1.0"),
                retargeted,
                skill("new", "claude", "0.1.0"),
                skill("drifty", "claude", "1.0.0"),
                skill("healed", "claude", "1.0.0"),
            ]),
            json!([
                status("drifty", "claude", "modified"),
                status("healed", "claude", "ok"),
            ]),
        ));
        let events = diff(Some(&previous), &current);
        let got: Vec<(&str, &str, Option<&str>)> = events
            .iter()
            .map(|event| {
                (
                    event.kind,
                    event.capability_id.as_deref().unwrap(),
                    event.target.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                (kind::DRIFT_DETECTED, "drifty", Some("claude")),
                (kind::DRIFT_CLEARED, "healed", Some("claude")),
                (kind::TARGET_ADDED, "lint", Some("cursor")),
                (kind::TARGET_REMOVED, "lint", Some("codex")),
                (kind::VERSION_CHANGED, "lint", Some("claude")),
                (kind::CAPABILITY_ADDED, "new", Some("claude")),
                (kind::CAPABILITY_REMOVED, "gone", Some("claude")),
            ]
        );
        let version = events
            .iter()
            .find(|event| event.kind == kind::VERSION_CHANGED)
            .unwrap();
        assert_eq!(version.detail.as_deref(), Some("1.0.0 to 1.1.0"));
    }

    #[test]
    fn a_new_ref_with_the_same_version_is_a_version_change() {
        let previous =
            Snapshot::from_report(&report(json!([skill("lint", "claude", "abc")]), json!([])));
        let mut moved = skill("lint", "claude", "abc");
        moved["source"]["ref"] = json!("def");
        let current = Snapshot::from_report(&report(json!([moved]), json!([])));
        let events = diff(Some(&previous), &current);
        assert_eq!(kinds(&events), [kind::VERSION_CHANGED]);
        assert!(
            events[0]
                .detail
                .as_deref()
                .unwrap()
                .starts_with("source git:")
        );
    }

    #[test]
    fn one_update_for_several_targets_is_one_event() {
        let previous = Snapshot::from_report(&report(
            json!([skill("lint", "claude", "1"), skill("lint", "codex", "1")]),
            json!([]),
        ));
        let current = Snapshot::from_report(&report(
            json!([skill("lint", "claude", "2"), skill("lint", "codex", "2")]),
            json!([]),
        ));
        let events = diff(Some(&previous), &current);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].target.as_deref(), Some("claude, codex"));
    }

    #[test]
    fn policy_gaps_open_and_close() {
        let with_gap = |rule: usize| {
            let mut policy = json!({
                "name": "guard", "type": "policy", "target": "codex", "version": "1",
                "source": { "kind": "local", "path": "p" },
            });
            policy["unenforced_rules"] = json!([{ "rule": rule, "description": "deny read \".env\"", "reason": "no native rule" }]);
            Snapshot::from_report(&report(json!([policy]), json!([])))
        };
        let added = diff(Some(&Snapshot::default()), &with_gap(2));
        assert!(kinds(&added).contains(&kind::POLICY_GAP_ADDED));
        let closed = diff(Some(&with_gap(2)), &Snapshot::default());
        assert!(kinds(&closed).contains(&kind::POLICY_GAP_CLOSED));
        assert!(diff(Some(&with_gap(2)), &with_gap(2)).is_empty());
    }
}
