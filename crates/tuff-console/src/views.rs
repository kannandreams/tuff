//! The read side of the API (RFC-108 D9): what the latest report of every
//! project says, grouped the way each UI view needs it.
//!
//! Everything here is computed from the latest stored report of each
//! project. A console holds one row per project for this, so the work grows
//! with the number of projects and not with the number of reports.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{Value, json};

use crate::events::{Gap, InventoryRow, Snapshot, is_ok};
use crate::store::ProjectRow;

/// Harnesses in the order the matrix lists them. Others follow
/// alphabetically.
const HARNESS_ORDER: &[&str] = &["claude", "codex", "cursor", "opencode", "open-agents"];

/// The newest version `tuff outdated` found for one capability and target.
#[derive(Debug, Clone)]
struct Newer {
    latest: Option<String>,
}

/// One project as the latest report describes it.
#[derive(Debug, Clone)]
pub struct ProjectModel {
    row: ProjectRow,
    commit: Option<String>,
    branch: Option<String>,
    dirty: bool,
    snapshot: Snapshot,
    outdated: BTreeMap<(String, String, String), Newer>,
}

/// One capability installed for one target, with what is known about its
/// state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CapabilityRow {
    #[serde(rename = "type")]
    capability_type: String,
    id: String,
    target: String,
    version: String,
    source: String,
    status: String,
    outdated: bool,
    latest: Option<String>,
}

impl ProjectModel {
    pub fn new(row: ProjectRow, report: &Value) -> Self {
        let outdated = report
            .get("outdated")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|entry| entry.get("status").and_then(Value::as_str) == Some("outdated"))
            .map(|entry| {
                let text = |key: &str| {
                    entry
                        .get(key)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string()
                };
                (
                    (text("type"), text("id"), text("target")),
                    Newer {
                        latest: entry
                            .get("latest")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    },
                )
            })
            .collect();
        let text = |pointer: &str| {
            report
                .pointer(pointer)
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        Self {
            row,
            commit: text("/project/commit"),
            branch: text("/project/branch"),
            dirty: report
                .pointer("/project/dirty")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            snapshot: Snapshot::from_report(report),
            outdated,
        }
    }

    fn capabilities(&self) -> Vec<CapabilityRow> {
        self.snapshot
            .rows
            .iter()
            .map(|row| self.capability_row(row))
            .collect()
    }

    fn capability_row(&self, row: &InventoryRow) -> CapabilityRow {
        let newer = self.outdated.get(&(
            row.capability_type.clone(),
            row.capability_id.clone(),
            row.target.clone(),
        ));
        CapabilityRow {
            capability_type: row.capability_type.clone(),
            id: row.capability_id.clone(),
            target: row.target.clone(),
            version: row.version.clone(),
            source: row.source.clone(),
            status: row.status.clone(),
            outdated: newer.is_some(),
            latest: newer.and_then(|newer| newer.latest.clone()),
        }
    }

    fn harnesses(&self) -> Vec<String> {
        let set: BTreeSet<&str> = self
            .snapshot
            .rows
            .iter()
            .map(|row| row.target.as_str())
            .collect();
        order_harnesses(set)
    }

    fn distinct_capabilities(&self) -> usize {
        self.snapshot
            .rows
            .iter()
            .map(|row| (&row.capability_type, &row.capability_id))
            .collect::<BTreeSet<_>>()
            .len()
    }

    /// Capabilities with a target whose files no longer match.
    fn drifted(&self) -> usize {
        self.snapshot
            .rows
            .iter()
            .filter(|row| !is_ok(&row.status))
            .map(|row| (&row.capability_type, &row.capability_id))
            .collect::<BTreeSet<_>>()
            .len()
    }

    fn outdated_count(&self) -> usize {
        self.outdated
            .keys()
            .map(|(kind, id, _)| (kind, id))
            .collect::<BTreeSet<_>>()
            .len()
    }

    /// `drift`, `gap`, `outdated`, or `ok`, worst first.
    fn status(&self) -> &'static str {
        if self.drifted() > 0 {
            "drift"
        } else if !self.snapshot.gaps.is_empty() {
            "gap"
        } else if !self.outdated.is_empty() {
            "outdated"
        } else {
            "ok"
        }
    }

    fn identity(&self) -> Value {
        json!({
            "id": self.row.id,
            "name": self.row.name,
            "repository": self.row.repository,
            "path": self.row.path,
        })
    }

    fn list_item(&self) -> Value {
        json!({
            "id": self.row.id,
            "repository": self.row.repository,
            "path": self.row.path,
            "name": self.row.name,
            "firstReportAt": self.row.first_report_at,
            "lastReportAt": self.row.last_report_at,
            "reportCount": self.row.report_count,
            "commit": self.commit,
            "branch": self.branch,
            "dirty": self.dirty,
            "harnesses": self.harnesses(),
            "capabilityCount": self.distinct_capabilities(),
            "driftCount": self.drifted(),
            "outdatedCount": self.outdated_count(),
            "policyGapCount": self.snapshot.gaps.len(),
            "status": self.status(),
        })
    }
}

fn order_harnesses<'a>(set: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut names: Vec<&str> = set.into_iter().collect();
    names.sort_by_key(|name| {
        (
            HARNESS_ORDER
                .iter()
                .position(|known| known == name)
                .unwrap_or(HARNESS_ORDER.len()),
            *name,
        )
    });
    names.dedup();
    names.into_iter().map(str::to_string).collect()
}

/// `GET /projects`: each project with its summary, and the totals the
/// Dashboard shows.
pub fn projects(models: &[ProjectModel]) -> Value {
    let capabilities: BTreeSet<(&str, &str)> = models
        .iter()
        .flat_map(|model| &model.snapshot.rows)
        .map(|row| (row.capability_type.as_str(), row.capability_id.as_str()))
        .collect();
    let harnesses: BTreeSet<&str> = models
        .iter()
        .flat_map(|model| &model.snapshot.rows)
        .map(|row| row.target.as_str())
        .collect();
    let repositories: BTreeSet<&str> = models
        .iter()
        .map(|model| model.row.repository.as_str())
        .collect();
    let last_report_at = models.iter().map(|model| &model.row.last_report_at).max();
    json!({
        "projects": models.iter().map(ProjectModel::list_item).collect::<Vec<_>>(),
        "summary": {
            "projects": models.len(),
            "repositories": repositories.len(),
            "capabilities": capabilities.len(),
            "harnesses": harnesses.len(),
            "driftCount": models.iter().map(ProjectModel::drifted).sum::<usize>(),
            "outdatedCount": models.iter().map(ProjectModel::outdated_count).sum::<usize>(),
            "policyGapCount": models.iter().map(|model| model.snapshot.gaps.len()).sum::<usize>(),
            "lastReportAt": last_report_at,
        },
    })
}

fn gap_json(model: &ProjectModel, gap: &Gap) -> Value {
    json!({
        "projectId": model.row.id,
        "name": model.row.name,
        "repository": model.row.repository,
        "path": model.row.path,
        "policy": gap.policy,
        "target": gap.target,
        "rule": gap.rule,
        "description": gap.description,
        "reason": gap.reason,
    })
}

/// `GET /projects/{id}`: one project, its capabilities per target, its
/// recorded policy gaps, and its latest report as stored.
pub fn project(model: &ProjectModel, latest_report: Value) -> Value {
    json!({
        "project": model.list_item(),
        "capabilities": model.capabilities(),
        "policyGaps": model
            .snapshot
            .gaps
            .iter()
            .map(|gap| gap_json(model, gap))
            .collect::<Vec<_>>(),
        "latestReport": latest_report,
    })
}

struct VersionCount {
    version: String,
    projects: BTreeSet<i64>,
}

fn version_counts<'a>(rows: impl Iterator<Item = (i64, &'a str)>) -> (Vec<Value>, bool) {
    let mut by_version: BTreeMap<&str, BTreeSet<i64>> = BTreeMap::new();
    for (project, version) in rows {
        by_version.entry(version).or_default().insert(project);
    }
    let mut counts: Vec<VersionCount> = by_version
        .into_iter()
        .map(|(version, projects)| VersionCount {
            version: version.to_string(),
            projects,
        })
        .collect();
    counts.sort_by(|a, b| {
        b.projects
            .len()
            .cmp(&a.projects.len())
            .then_with(|| a.version.cmp(&b.version))
    });
    let mixed = counts.len() > 1;
    (
        counts
            .into_iter()
            .map(|count| json!({ "version": count.version, "projects": count.projects.len() }))
            .collect(),
        mixed,
    )
}

/// `GET /capabilities`: every capability across projects, with the
/// versions in use. `type_filter` keeps one capability type.
pub fn capabilities(models: &[ProjectModel], type_filter: Option<&str>) -> Value {
    type Key<'a> = (&'a str, &'a str);
    let mut grouped: BTreeMap<Key<'_>, Vec<(&ProjectModel, &InventoryRow)>> = BTreeMap::new();
    for model in models {
        for row in &model.snapshot.rows {
            if type_filter.is_some_and(|wanted| wanted != row.capability_type) {
                continue;
            }
            grouped
                .entry((row.capability_type.as_str(), row.capability_id.as_str()))
                .or_default()
                .push((model, row));
        }
    }
    let mut items: Vec<(usize, Value)> = grouped
        .into_iter()
        .map(|((kind, id), uses)| {
            let projects: BTreeMap<i64, &ProjectModel> = uses
                .iter()
                .map(|(model, _)| (model.row.id, *model))
                .collect();
            let (versions, mixed) = version_counts(
                uses.iter()
                    .map(|(model, row)| (model.row.id, row.version.as_str())),
            );
            let targets: BTreeSet<&str> = uses.iter().map(|(_, row)| row.target.as_str()).collect();
            (
                projects.len(),
                json!({
                    "type": kind,
                    "id": id,
                    "versions": versions,
                    "mixed": mixed,
                    "projectCount": projects.len(),
                    "targets": order_harnesses(targets),
                    "projects": projects.values().map(|model| model.identity()).collect::<Vec<_>>(),
                }),
            )
        })
        .collect();
    items.sort_by(|a, b| {
        b.0.cmp(&a.0).then_with(|| {
            let key = |value: &Value| {
                (
                    value["type"].as_str().unwrap_or_default().to_string(),
                    value["id"].as_str().unwrap_or_default().to_string(),
                )
            };
            key(&a.1).cmp(&key(&b.1))
        })
    });
    let types: BTreeSet<&str> = models
        .iter()
        .flat_map(|model| &model.snapshot.rows)
        .map(|row| row.capability_type.as_str())
        .collect();
    json!({
        "capabilities": items.into_iter().map(|(_, value)| value).collect::<Vec<_>>(),
        "types": types,
    })
}

/// `GET /capabilities/{type}/{id}`: where one capability is used.
pub fn capability(models: &[ProjectModel], capability_type: &str, id: &str) -> Option<Value> {
    let mut usage = Vec::new();
    let mut versions = Vec::new();
    for model in models {
        for row in &model.snapshot.rows {
            if row.capability_type == capability_type && row.capability_id == id {
                versions.push((model.row.id, row.version.as_str()));
                let detail = model.capability_row(row);
                usage.push(json!({
                    "projectId": model.row.id,
                    "name": model.row.name,
                    "repository": model.row.repository,
                    "path": model.row.path,
                    "target": detail.target,
                    "version": detail.version,
                    "source": detail.source,
                    "status": detail.status,
                    "outdated": detail.outdated,
                    "latest": detail.latest,
                }));
            }
        }
    }
    if usage.is_empty() {
        return None;
    }
    let (version_list, mixed) = version_counts(versions.into_iter());
    let project_count = usage
        .iter()
        .filter_map(|entry| entry["projectId"].as_i64())
        .collect::<BTreeSet<_>>()
        .len();
    Some(json!({
        "type": capability_type,
        "id": id,
        "versions": version_list,
        "mixed": mixed,
        "projectCount": project_count,
        "usage": usage,
    }))
}

/// `GET /harnesses`: the project by harness matrix. A cell is the number of
/// capabilities the project has installed for that harness.
pub fn harnesses(models: &[ProjectModel]) -> Value {
    let all: BTreeSet<&str> = models
        .iter()
        .flat_map(|model| &model.snapshot.rows)
        .map(|row| row.target.as_str())
        .collect();
    let names = order_harnesses(all);
    let rows: Vec<Value> = models
        .iter()
        .map(|model| {
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            for row in &model.snapshot.rows {
                *counts.entry(row.target.as_str()).or_default() += 1;
            }
            let mut identity = model.identity();
            identity["counts"] = json!(counts);
            identity
        })
        .collect();
    let totals: BTreeMap<&str, usize> = names
        .iter()
        .map(|name| {
            (
                name.as_str(),
                models
                    .iter()
                    .filter(|model| model.snapshot.rows.iter().any(|row| row.target == *name))
                    .count(),
            )
        })
        .collect();
    json!({ "harnesses": names, "projects": rows, "totals": totals })
}

/// `GET /policies`: who carries which policy, who carries none, and every
/// rule recorded as not enforced.
pub fn policies(models: &[ProjectModel]) -> Value {
    let mut grouped: BTreeMap<&str, Vec<(&ProjectModel, Vec<&InventoryRow>)>> = BTreeMap::new();
    for model in models {
        let mut own: BTreeMap<&str, Vec<&InventoryRow>> = BTreeMap::new();
        for row in &model.snapshot.rows {
            if row.capability_type == "policy" {
                own.entry(row.capability_id.as_str()).or_default().push(row);
            }
        }
        for (id, rows) in own {
            grouped.entry(id).or_default().push((model, rows));
        }
    }
    let policies: Vec<Value> = grouped
        .into_iter()
        .map(|(id, carriers)| {
            let (versions, mixed) = version_counts(carriers.iter().flat_map(|(model, rows)| {
                rows.iter().map(|row| (model.row.id, row.version.as_str()))
            }));
            let usage: Vec<Value> = carriers
                .iter()
                .map(|(model, rows)| {
                    let mut identity = model.identity();
                    identity["version"] = json!(rows.first().map(|row| row.version.as_str()));
                    identity["targets"] =
                        json!(order_harnesses(rows.iter().map(|row| row.target.as_str())));
                    let newer = rows
                        .iter()
                        .map(|row| model.capability_row(row))
                        .find(|detail| detail.outdated);
                    identity["outdated"] = json!(newer.is_some());
                    identity["latest"] = json!(newer.and_then(|detail| detail.latest));
                    identity["gapCount"] = json!(
                        model
                            .snapshot
                            .gaps
                            .iter()
                            .filter(|gap| gap.policy == id)
                            .count()
                    );
                    identity
                })
                .collect();
            json!({
                "id": id,
                "projectCount": carriers.len(),
                "versions": versions,
                "mixed": mixed,
                "usage": usage,
            })
        })
        .collect();
    let without: Vec<Value> = models
        .iter()
        .filter(|model| {
            !model
                .snapshot
                .rows
                .iter()
                .any(|row| row.capability_type == "policy")
        })
        .map(|model| {
            let mut identity = model.identity();
            identity["harnesses"] = json!(model.harnesses());
            identity
        })
        .collect();
    let gaps: Vec<Value> = models
        .iter()
        .flat_map(|model| model.snapshot.gaps.iter().map(|gap| gap_json(model, gap)))
        .collect();
    json!({ "policies": policies, "projectsWithoutPolicy": without, "gaps": gaps })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: i64, name: &str, report: Value) -> ProjectModel {
        ProjectModel::new(
            ProjectRow {
                id,
                repository: "github.com/acme/web".into(),
                path: name.into(),
                name: name.into(),
                first_report_at: "2026-09-30T10:00:00Z".into(),
                last_report_at: "2026-09-30T11:00:00Z".into(),
                report_count: 1,
            },
            &report,
        )
    }

    fn row(name: &str, kind: &str, target: &str, version: &str) -> Value {
        json!({
            "name": name, "type": kind, "target": target, "version": version,
            "source": { "kind": "local", "path": name },
        })
    }

    fn report(rows: Vec<Value>, statuses: Vec<Value>, outdated: Value) -> Value {
        json!({
            "project": { "commit": "abc", "branch": "main", "dirty": true },
            "lockfile": { "version": 3, "capabilities": rows },
            "check": { "valid": true, "results": statuses },
            "outdated": outdated,
        })
    }

    fn fixture() -> Vec<ProjectModel> {
        let mut guard = row("guard", "policy", "codex", "1.1.0");
        guard["unenforced_rules"] =
            json!([{ "rule": 3, "description": "deny edit", "reason": "no native rule" }]);
        vec![
            model(
                1,
                "checkout",
                report(
                    vec![
                        row("review", "skill", "claude", "2.3.0"),
                        row("review", "skill", "cursor", "2.3.0"),
                        guard,
                    ],
                    vec![
                        json!({ "id": "review", "type": "skill", "target": "cursor", "status": "modified" }),
                    ],
                    json!([{ "id": "guard", "type": "policy", "target": "codex", "current": "1.1.0", "latest": "1.2.0", "status": "outdated" }]),
                ),
            ),
            model(
                2,
                "admin",
                report(
                    vec![row("review", "skill", "claude", "2.1.0")],
                    vec![],
                    Value::Null,
                ),
            ),
        ]
    }

    #[test]
    fn projects_carry_a_status_and_counts() {
        let value = projects(&fixture());
        let list = value["projects"].as_array().unwrap();
        assert_eq!(list[0]["status"], "drift");
        assert_eq!(list[0]["driftCount"], 1);
        assert_eq!(list[0]["capabilityCount"], 2);
        assert_eq!(list[0]["policyGapCount"], 1);
        assert_eq!(list[0]["outdatedCount"], 1);
        assert_eq!(list[0]["harnesses"], json!(["claude", "codex", "cursor"]));
        assert_eq!(list[0]["dirty"], true);
        assert_eq!(list[1]["status"], "ok");
        assert_eq!(value["summary"]["projects"], 2);
        assert_eq!(value["summary"]["repositories"], 1);
        assert_eq!(value["summary"]["capabilities"], 2);
        assert_eq!(value["summary"]["driftCount"], 1);
    }

    #[test]
    fn capabilities_group_versions_and_flag_mixed_ones() {
        let value = capabilities(&fixture(), None);
        let list = value["capabilities"].as_array().unwrap();
        let review = list.iter().find(|item| item["id"] == "review").unwrap();
        assert_eq!(review["mixed"], true);
        assert_eq!(review["projectCount"], 2);
        assert_eq!(review["versions"][0]["version"], "2.1.0");
        assert_eq!(review["versions"][0]["projects"], 1);
        assert_eq!(value["types"], json!(["policy", "skill"]));
        let policies_only = capabilities(&fixture(), Some("policy"));
        assert_eq!(policies_only["capabilities"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn one_capability_lists_where_it_is_used() {
        let models = fixture();
        let value = capability(&models, "skill", "review").unwrap();
        assert_eq!(value["usage"].as_array().unwrap().len(), 3);
        assert_eq!(value["projectCount"], 2);
        assert!(capability(&models, "skill", "missing").is_none());
        let guard = capability(&models, "policy", "guard").unwrap();
        assert_eq!(guard["usage"][0]["outdated"], true);
        assert_eq!(guard["usage"][0]["latest"], "1.2.0");
    }

    #[test]
    fn the_harness_matrix_counts_capabilities_per_cell() {
        let value = harnesses(&fixture());
        assert_eq!(value["harnesses"], json!(["claude", "codex", "cursor"]));
        assert_eq!(value["projects"][0]["counts"]["claude"], 1);
        assert_eq!(value["projects"][1]["counts"]["codex"], Value::Null);
        assert_eq!(value["totals"]["claude"], 2);
    }

    #[test]
    fn policies_list_carriers_gaps_and_projects_without_one() {
        let value = policies(&fixture());
        assert_eq!(value["policies"][0]["id"], "guard");
        assert_eq!(value["policies"][0]["usage"][0]["gapCount"], 1);
        assert_eq!(value["projectsWithoutPolicy"][0]["name"], "admin");
        assert_eq!(value["gaps"][0]["rule"], 3);
        assert_eq!(value["gaps"][0]["target"], "codex");
    }

    #[test]
    fn a_project_shows_capabilities_per_target() {
        let models = fixture();
        let value = project(&models[0], json!({ "schema": 1 }));
        assert_eq!(value["capabilities"].as_array().unwrap().len(), 3);
        assert_eq!(value["capabilities"][2]["status"], "modified");
        assert_eq!(value["policyGaps"][0]["policy"], "guard");
        assert_eq!(value["latestReport"]["schema"], 1);
    }
}
