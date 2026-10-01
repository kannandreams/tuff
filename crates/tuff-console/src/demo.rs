//! Sample projects for `tuff console serve --demo`.
//!
//! The reports are generated here and stored through [`Store::ingest_at`],
//! the path `POST /api/v1/reports` uses, so the events and the inventory
//! come from the same code as real data. Each project has a history of
//! reports spread over the last days, which gives the audit log entries of
//! every kind.

use chrono::{Duration, SecondsFormat, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tuff_core::error::Result;
use tuff_core::report::{REPORT_SCHEMA, Report};

use crate::store::Store;

struct Cap {
    name: &'static str,
    kind: &'static str,
    version: &'static str,
    targets: &'static [&'static str],
    /// The `tuff check` status of every target of this capability.
    status: &'static str,
}

#[derive(Clone, Copy)]
struct Gap {
    policy: &'static str,
    target: &'static str,
    rule: u64,
    description: &'static str,
    reason: &'static str,
}

struct Newer {
    name: &'static str,
    kind: &'static str,
    latest: &'static str,
}

struct Step {
    minutes_ago: i64,
    commit: &'static str,
    dirty: bool,
    caps: Vec<Cap>,
    gaps: Vec<Gap>,
    newer: Vec<Newer>,
}

struct Spec {
    repository: &'static str,
    path: &'static str,
    name: &'static str,
    steps: Vec<Step>,
}

const fn cap(
    name: &'static str,
    kind: &'static str,
    version: &'static str,
    targets: &'static [&'static str],
) -> Cap {
    Cap {
        name,
        kind,
        version,
        targets,
        status: "ok",
    }
}

const fn drifted(
    name: &'static str,
    kind: &'static str,
    version: &'static str,
    targets: &'static [&'static str],
) -> Cap {
    Cap {
        name,
        kind,
        version,
        targets,
        status: "modified",
    }
}

const MINUTE: i64 = 1;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

fn specs() -> Vec<Spec> {
    const CLAUDE_CODEX: &[&str] = &["claude", "codex"];
    const CLAUDE_CURSOR: &[&str] = &["claude", "cursor"];
    const CLAUDE: &[&str] = &["claude"];
    const CODEX: &[&str] = &["codex"];
    const CODEX_AGENTS: &[&str] = &["codex", "open-agents"];
    const CURSOR_CLAUDE: &[&str] = &["cursor", "claude"];

    let mcp_gap = Gap {
        policy: "infra-guardrails",
        target: "codex",
        rule: 4,
        description: "deny mcp \"github:delete_*\"",
        reason: "Codex names MCP tools exactly, so a pattern with '*' has no Codex form",
    };
    let cursor_gap = Gap {
        policy: "infra-guardrails",
        target: "cursor",
        rule: 3,
        description: "deny edit \"secrets/**\"",
        reason: "Cursor's preToolUse input does not document the path of a file write",
    };
    let guardrails_old_gap = Gap {
        policy: "infra-guardrails",
        target: "codex",
        rule: 2,
        description: "ask bash \"terraform apply*\"",
        reason: "Codex has no per-command approval rule for this pattern",
    };

    vec![
        Spec {
            repository: "github.com/acme/payments-api",
            path: ".",
            name: "payments-api",
            steps: vec![
                Step {
                    minutes_ago: 9 * DAY,
                    commit: "1a9e0b3",
                    dirty: false,
                    caps: vec![
                        cap("code-review", "skill", "2.2.0", CLAUDE_CODEX),
                        cap("github", "mcp-server", "0.6.0", CLAUDE_CODEX),
                        cap("postgres", "mcp-server", "1.3.0", CLAUDE_CODEX),
                        cap("pre-commit-lint", "hook", "0.9.0", CLAUDE_CODEX),
                        cap("infra-guardrails", "policy", "1.2.0", CLAUDE_CODEX),
                    ],
                    gaps: vec![mcp_gap],
                    newer: vec![Newer {
                        name: "postgres",
                        kind: "mcp-server",
                        latest: "1.4.2",
                    }],
                },
                Step {
                    minutes_ago: 3 * DAY,
                    commit: "7c31d44",
                    dirty: false,
                    caps: vec![
                        cap("code-review", "skill", "2.2.0", CLAUDE_CODEX),
                        cap("release-notes", "skill", "1.1.0", CLAUDE_CODEX),
                        cap("github", "mcp-server", "0.6.0", CLAUDE_CODEX),
                        cap("postgres", "mcp-server", "1.3.0", CLAUDE_CODEX),
                        cap("pre-commit-lint", "hook", "0.9.0", CLAUDE),
                        cap("infra-guardrails", "policy", "1.2.0", CLAUDE_CODEX),
                    ],
                    gaps: vec![mcp_gap],
                    newer: vec![Newer {
                        name: "postgres",
                        kind: "mcp-server",
                        latest: "1.4.2",
                    }],
                },
                Step {
                    minutes_ago: 4 * MINUTE,
                    commit: "4f2a91c",
                    dirty: false,
                    caps: vec![
                        cap("code-review", "skill", "2.3.0", CLAUDE_CODEX),
                        cap("release-notes", "skill", "1.1.0", CLAUDE_CODEX),
                        cap("github", "mcp-server", "0.6.0", CLAUDE_CODEX),
                        cap("postgres", "mcp-server", "1.3.0", CLAUDE_CODEX),
                        cap("pre-commit-lint", "hook", "0.9.0", CLAUDE),
                        cap("infra-guardrails", "policy", "1.2.0", CLAUDE_CODEX),
                    ],
                    gaps: vec![mcp_gap],
                    newer: vec![Newer {
                        name: "postgres",
                        kind: "mcp-server",
                        latest: "1.4.2",
                    }],
                },
            ],
        },
        Spec {
            repository: "github.com/acme/web",
            path: "apps/checkout",
            name: "checkout",
            steps: vec![
                Step {
                    minutes_ago: 6 * DAY,
                    commit: "55c0a1e",
                    dirty: false,
                    caps: vec![
                        cap("code-review", "skill", "2.3.0", CLAUDE_CURSOR),
                        cap("frontend-design", "skill", "1.0.4", CLAUDE_CURSOR),
                        cap("infra-guardrails", "policy", "1.2.0", CLAUDE_CURSOR),
                        cap("format-on-save", "hook", "1.0.0", CLAUDE),
                    ],
                    gaps: vec![],
                    newer: vec![],
                },
                Step {
                    minutes_ago: 38 * MINUTE,
                    commit: "b81d0e7",
                    dirty: false,
                    caps: vec![
                        cap("code-review", "skill", "2.3.0", CLAUDE_CURSOR),
                        cap("frontend-design", "skill", "1.0.4", CLAUDE_CURSOR),
                        cap("figma", "mcp-server", "0.3.1", CLAUDE_CURSOR),
                        cap("infra-guardrails", "policy", "1.2.0", CLAUDE_CURSOR),
                        drifted("format-on-save", "hook", "1.0.0", CLAUDE),
                    ],
                    gaps: vec![],
                    newer: vec![],
                },
            ],
        },
        Spec {
            repository: "github.com/acme/web",
            path: "apps/admin",
            name: "admin",
            steps: vec![
                Step {
                    minutes_ago: 6 * DAY,
                    commit: "55c0a1e",
                    dirty: false,
                    caps: vec![
                        cap("code-review", "skill", "2.1.0", CLAUDE),
                        cap("frontend-design", "skill", "1.0.4", CLAUDE),
                        cap("github", "mcp-server", "0.6.0", CLAUDE),
                    ],
                    gaps: vec![],
                    newer: vec![Newer {
                        name: "code-review",
                        kind: "skill",
                        latest: "2.3.0",
                    }],
                },
                Step {
                    minutes_ago: 38 * MINUTE,
                    commit: "b81d0e7",
                    dirty: false,
                    caps: vec![
                        cap("code-review", "skill", "2.1.0", CLAUDE),
                        cap("frontend-design", "skill", "1.0.4", CLAUDE),
                        cap("github", "mcp-server", "0.6.0", CLAUDE),
                    ],
                    gaps: vec![],
                    newer: vec![Newer {
                        name: "code-review",
                        kind: "skill",
                        latest: "2.3.0",
                    }],
                },
            ],
        },
        Spec {
            repository: "github.com/acme/platform-infra",
            path: ".",
            name: "platform-infra",
            steps: vec![
                Step {
                    minutes_ago: 5 * DAY,
                    commit: "2f7d8c0",
                    dirty: false,
                    caps: vec![
                        cap("terraform-plan-review", "skill", "0.8.0", CLAUDE_CODEX),
                        cap("infra-guardrails", "policy", "1.2.0", CLAUDE_CODEX),
                        drifted("aws-docs", "mcp-server", "2.0.0", CLAUDE_CODEX),
                        cap("github", "mcp-server", "0.6.0", CLAUDE_CODEX),
                    ],
                    gaps: vec![guardrails_old_gap],
                    newer: vec![],
                },
                Step {
                    minutes_ago: DAY,
                    commit: "3be19d4",
                    dirty: false,
                    caps: vec![
                        cap("terraform-plan-review", "skill", "0.8.0", CLAUDE_CODEX),
                        cap(
                            "infra-guardrails",
                            "policy",
                            "1.2.0",
                            &["claude", "codex", "opencode"],
                        ),
                        cap("aws-docs", "mcp-server", "2.0.0", CLAUDE_CODEX),
                        cap("github", "mcp-server", "0.6.0", CLAUDE_CODEX),
                    ],
                    gaps: vec![guardrails_old_gap],
                    newer: vec![],
                },
                Step {
                    minutes_ago: 2 * HOUR,
                    commit: "9c0e3aa",
                    dirty: false,
                    caps: vec![
                        cap("terraform-plan-review", "skill", "0.8.0", CLAUDE_CODEX),
                        cap(
                            "infra-guardrails",
                            "policy",
                            "1.2.0",
                            &["claude", "codex", "opencode"],
                        ),
                        cap("aws-docs", "mcp-server", "2.0.0", CLAUDE_CODEX),
                        cap("github", "mcp-server", "0.6.0", CLAUDE_CODEX),
                    ],
                    gaps: vec![],
                    newer: vec![],
                },
            ],
        },
        Spec {
            repository: "github.com/acme/data-pipeline",
            path: ".",
            name: "data-pipeline",
            steps: vec![
                Step {
                    minutes_ago: 8 * DAY,
                    commit: "c8a1b77",
                    dirty: false,
                    caps: vec![
                        cap("sql-style", "skill", "1.3.0", CODEX_AGENTS),
                        cap("postgres", "mcp-server", "1.3.0", CODEX_AGENTS),
                        cap("dbt-docs", "tool", "0.2.0", CODEX),
                    ],
                    gaps: vec![],
                    newer: vec![Newer {
                        name: "postgres",
                        kind: "mcp-server",
                        latest: "1.4.2",
                    }],
                },
                Step {
                    minutes_ago: 26 * HOUR,
                    commit: "e57b212",
                    dirty: true,
                    caps: vec![
                        cap("sql-style", "skill", "1.3.0", CODEX_AGENTS),
                        cap("postgres", "mcp-server", "1.3.0", CODEX_AGENTS),
                        drifted("dbt-docs", "tool", "0.2.0", CODEX),
                    ],
                    gaps: vec![],
                    newer: vec![Newer {
                        name: "postgres",
                        kind: "mcp-server",
                        latest: "1.4.2",
                    }],
                },
            ],
        },
        Spec {
            repository: "github.com/acme/mobile",
            path: ".",
            name: "mobile",
            steps: vec![
                Step {
                    minutes_ago: 3 * DAY,
                    commit: "71aa0fd",
                    dirty: false,
                    caps: vec![
                        cap("code-review", "skill", "2.3.0", CURSOR_CLAUDE),
                        cap("infra-guardrails", "policy", "1.1.0", CURSOR_CLAUDE),
                        cap("github", "mcp-server", "0.6.0", CURSOR_CLAUDE),
                        cap("pre-commit-lint", "hook", "0.9.0", CLAUDE),
                    ],
                    gaps: vec![cursor_gap],
                    newer: vec![Newer {
                        name: "infra-guardrails",
                        kind: "policy",
                        latest: "1.2.0",
                    }],
                },
                Step {
                    minutes_ago: 2 * DAY,
                    commit: "0d4c8e1",
                    dirty: false,
                    caps: vec![
                        cap("code-review", "skill", "2.3.0", CURSOR_CLAUDE),
                        cap("infra-guardrails", "policy", "1.1.0", CURSOR_CLAUDE),
                        cap("github", "mcp-server", "0.6.0", CURSOR_CLAUDE),
                    ],
                    gaps: vec![cursor_gap],
                    newer: vec![Newer {
                        name: "infra-guardrails",
                        kind: "policy",
                        latest: "1.2.0",
                    }],
                },
            ],
        },
    ]
}

fn digest(text: &str) -> String {
    use std::fmt::Write;
    Sha256::digest(text.as_bytes())
        .iter()
        .fold(String::new(), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// The report a project would publish at one step.
fn report(spec: &Spec, step: &Step, generated_at: &str) -> Value {
    let mut rows = Vec::new();
    let mut results = Vec::new();
    for cap in &step.caps {
        for target in cap.targets {
            let hash = digest(&format!("{}@{}", cap.name, cap.version));
            let mut row = json!({
                "name": cap.name,
                "type": cap.kind,
                "version": cap.version,
                "version_scheme": "semver",
                "description": "",
                "target": target,
                "installed_path": format!(".{target}/{}/{}", cap.kind, cap.name),
                "sha256": hash,
                "ownership": "generated",
                "source": {
                    "kind": "git",
                    "url": "https://example.com/acme/agent-capabilities",
                    "path": format!("{}/{}", cap.kind, cap.name),
                    "ref": &digest(&format!("ref {}@{}", cap.name, cap.version))[..40],
                    "tag": format!("{}-v{}", cap.name, cap.version),
                },
            });
            let rules: Vec<Value> = step
                .gaps
                .iter()
                .filter(|gap| gap.policy == cap.name && gap.target == *target)
                .map(|gap| {
                    json!({ "rule": gap.rule, "description": gap.description, "reason": gap.reason })
                })
                .collect();
            if !rules.is_empty() {
                row["unenforced_rules"] = Value::Array(rules);
            }
            rows.push(row);
            results.push(json!({
                "id": cap.name,
                "type": cap.kind,
                "target": target,
                "status": cap.status,
            }));
        }
    }
    let outdated: Vec<Value> = step
        .newer
        .iter()
        .flat_map(|newer| {
            step.caps
                .iter()
                .filter(|cap| cap.name == newer.name && cap.kind == newer.kind)
                .flat_map(move |cap| {
                    cap.targets.iter().map(move |target| {
                        json!({
                            "id": newer.name,
                            "type": newer.kind,
                            "target": target,
                            "version_scheme": "semver",
                            "current": cap.version,
                            "latest": newer.latest,
                            "status": "outdated",
                        })
                    })
                })
        })
        .collect();
    json!({
        "schema": REPORT_SCHEMA,
        "tuffVersion": env!("CARGO_PKG_VERSION"),
        "generatedAt": generated_at,
        "project": {
            "repository": spec.repository,
            "path": spec.path,
            "name": spec.name,
            "commit": step.commit,
            "branch": "main",
            "dirty": step.dirty,
        },
        "lockfile": { "version": 3, "capabilities": rows },
        "check": {
            "valid": results.iter().all(|result| result["status"] == "ok"),
            "results": results,
        },
        "outdated": outdated,
    })
}

/// Fill `store` with the sample projects, as if each had published its
/// reports at the times in their history.
pub fn populate(store: &Store) -> Result<()> {
    let specs = specs();
    let mut timeline: Vec<(i64, usize, usize)> = Vec::new();
    for (spec_index, spec) in specs.iter().enumerate() {
        for (step_index, step) in spec.steps.iter().enumerate() {
            timeline.push((step.minutes_ago, spec_index, step_index));
        }
    }
    // Oldest first, so event ids follow time.
    timeline.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let now = Utc::now();
    for (minutes_ago, spec_index, step_index) in timeline {
        let spec = &specs[spec_index];
        let at = (now - Duration::minutes(minutes_ago)).to_rfc3339_opts(SecondsFormat::Secs, true);
        let raw = report(spec, &spec.steps[step_index], &at);
        let parsed: Report = serde_json::from_value(raw.clone())?;
        store.ingest_at(&parsed, &raw, &at)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::EventFilter;

    #[test]
    fn the_sample_data_goes_through_ingest_and_yields_every_kind_of_event() {
        let store = Store::open_in_memory().unwrap();
        populate(&store).unwrap();
        assert_eq!(store.projects().unwrap().len(), 6);

        let events = store
            .events(&EventFilter {
                limit: Some(1000),
                ..Default::default()
            })
            .unwrap();
        for kind in crate::events::kind::ALL {
            assert!(
                events.iter().any(|event| event.kind == *kind),
                "the demo has no {kind} event"
            );
        }
        // Newest first, in time order.
        assert!(
            events
                .windows(2)
                .all(|pair| pair[0].occurred_at >= pair[1].occurred_at)
        );
    }

    #[test]
    fn the_sample_data_has_drift_outdated_versions_and_policy_gaps() {
        let store = Store::open_in_memory().unwrap();
        populate(&store).unwrap();
        let models: Vec<_> = store
            .latest_reports()
            .unwrap()
            .into_iter()
            .map(|(row, body)| crate::views::ProjectModel::new(row, &body))
            .collect();
        let summary = crate::views::projects(&models)["summary"].clone();
        assert_eq!(summary["projects"], 6);
        assert_eq!(summary["repositories"], 5);
        assert_eq!(summary["driftCount"], 2);
        assert!(summary["outdatedCount"].as_u64().unwrap() >= 3);
        assert_eq!(summary["policyGapCount"], 2);
    }

    #[test]
    fn admins_second_report_is_deduplicated_and_adds_no_events() {
        let store = Store::open_in_memory().unwrap();
        populate(&store).unwrap();
        let admin = store
            .projects()
            .unwrap()
            .into_iter()
            .find(|project| project.name == "admin")
            .unwrap();
        assert_eq!(
            admin.report_count, 1,
            "the second report differs only in its commit"
        );
        let events = store
            .events(&EventFilter {
                project_id: Some(admin.id),
                ..Default::default()
            })
            .unwrap();
        assert!(events.iter().all(|event| {
            event.kind == "project_first_seen" || event.kind == "capability_added"
        }));
    }
}
