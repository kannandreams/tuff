use serde::Serialize;
use tuff_core::policy::PolicyCoverageEntry;
use tuff_hooks_spec::CoverageLevel;

use crate::adapter::{AdapterKind, AgentAdapter};
use crate::error::Result;

use super::render_table;

/// One agent's policy matrix, as `tuff policy matrix --json` prints it.
#[derive(Serialize)]
struct AgentPolicyMatrix {
    adapter: &'static str,
    display_name: &'static str,
    rules: Vec<PolicyCoverageEntry>,
}

/// Print, for every agent, how each kind of policy rule is enforced.
///
/// Every agent is listed, registered in this project or not, because the
/// question this answers is what a policy would do before anyone installs
/// it. It needs no project.
pub fn cmd_policy_matrix(json: bool) -> Result<()> {
    let matrices: Vec<AgentPolicyMatrix> = AdapterKind::all()
        .into_iter()
        .map(|adapter| AgentPolicyMatrix {
            adapter: adapter.id(),
            display_name: adapter.display_name(),
            rules: adapter.policy_compatibility(),
        })
        .collect();

    if json {
        println!("{}", serde_json::to_string_pretty(&matrices)?);
        return Ok(());
    }

    let mut rows = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    for matrix in &matrices {
        for entry in &matrix.rules {
            rows.push(vec![
                matrix.adapter.to_string(),
                entry.effect.as_str().to_string(),
                entry.subject.as_str().to_string(),
                coverage_label(entry.coverage).to_string(),
                entry.mechanism.clone().unwrap_or_default(),
            ]);
            // One note per distinct caveat per agent: a harness Tuff does
            // not compile for says the same thing on every row.
            if let Some(caveat) = &entry.caveat {
                let note = format!("{}: {caveat}", matrix.adapter);
                if !notes.contains(&note) {
                    notes.push(note);
                }
            }
        }
    }

    println!(
        "{}",
        render_table(
            &["ADAPTER", "EFFECT", "SUBJECT", "COVERAGE", "MECHANISM"],
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

fn coverage_label(coverage: CoverageLevel) -> &'static str {
    match coverage {
        CoverageLevel::Full => "full",
        CoverageLevel::Partial => "partial",
        CoverageLevel::Unsupported => "unsupported",
    }
}
