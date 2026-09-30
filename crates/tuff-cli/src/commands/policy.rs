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

/// The largest hook input `tuff policy evaluate` reads.
const MAX_HOOK_INPUT: u64 = 8 * 1024 * 1024;

/// `tuff policy evaluate --harness <id> --policy <id>`: answer one hook
/// call and return the exit status. Everything the harness needs goes to
/// standard output in its own format; a call that cannot be judged, such
/// as unreadable input or a policy that is no longer installed, is denied.
pub fn cmd_policy_evaluate(harness: &str, policy_id: &str) -> i32 {
    use std::io::Write;
    use tuff_core::policy_eval::PolicyVerdict;

    let Some(adapter) = AdapterKind::from_id(harness) else {
        eprintln!("tuff policy evaluate: unknown harness '{harness}'");
        return 2;
    };
    let answer = match judge_hook_call(adapter, policy_id) {
        Ok((event, Some(decision))) => {
            adapter.policy_hook_answer(&event, PolicyVerdict::Matched(&decision))
        }
        Ok((event, None)) => adapter.policy_hook_answer(&event, PolicyVerdict::NoMatch),
        Err((event, error)) => {
            let mut message = format!("tuff policy evaluate refused the call: {}", error.message());
            if let Some(hint) = error.hint() {
                message.push_str(&format!(" ({hint})"));
            }
            eprintln!("{message}");
            adapter.policy_hook_answer(&event, PolicyVerdict::Failed(&message))
        }
    };
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(answer.stdout.as_bytes());
    let _ = stdout.flush();
    answer.exit_code
}

type Judged = std::result::Result<
    (String, Option<tuff_core::policy_eval::PolicyDecision>),
    (String, tuff_core::error::TuffError),
>;

fn judge_hook_call(adapter: AdapterKind, policy_id: &str) -> Judged {
    use std::io::Read;
    use tuff_core::error::TuffError;
    use tuff_core::policy_eval::{self, EvalContext};

    let default_event = match adapter {
        AdapterKind::Claude | AdapterKind::Codex => "PreToolUse",
        _ => "",
    };
    let fail = |event: &str| {
        let event = event.to_string();
        move |error: TuffError| (event.clone(), error)
    };
    if policy_id.is_empty()
        || !policy_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        || policy_id.starts_with('.')
    {
        return Err(fail(default_event)(TuffError::usage(format!(
            "'{}' is not a policy id",
            policy_id.escape_debug()
        ))));
    }
    let mut text = String::new();
    std::io::stdin()
        .take(MAX_HOOK_INPUT)
        .read_to_string(&mut text)
        .map_err(|error| TuffError::usage(format!("cannot read the hook input: {error}")))
        .map_err(fail(default_event))?;
    let input: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| TuffError::usage(format!("the hook input is not JSON: {error}")))
        .map_err(fail(default_event))?;
    let request = adapter
        .policy_hook_request(&input)
        .map_err(fail(default_event))?;
    let event = request.event.clone();

    let process_cwd = std::env::current_dir().ok();
    let mut starts: Vec<std::path::PathBuf> = Vec::new();
    if adapter == AdapterKind::Claude
        && let Some(dir) = std::env::var_os("CLAUDE_PROJECT_DIR")
    {
        starts.push(dir.into());
    }
    starts.extend(request.roots.iter().cloned());
    starts.extend(request.cwd.iter().cloned());
    starts.extend(process_cwd.iter().cloned());
    let root = policy_eval::find_policy_root(&starts, adapter.dir_prefix(), policy_id)
        .ok_or_else(|| {
            TuffError::not_found(format!(
                "policy '{policy_id}' is not installed for {} above {}",
                adapter.display_name(),
                starts.first().map_or_else(
                    || "the working directory".to_string(),
                    |dir| dir.display().to_string()
                )
            ))
            .with_hint(format!(
                "run 'tuff check' in the project; 'tuff delete {policy_id}' removes this hook"
            ))
        })
        .map_err(fail(&event))?;
    let record = root
        .join(adapter.dir_prefix())
        .join("policies")
        .join(policy_id)
        .join("policy.toml");
    let policy = policy_eval::load_installed_policy(&record).map_err(fail(&event))?;
    let cwd = request
        .cwd
        .clone()
        .or(process_cwd)
        .unwrap_or_else(|| root.clone());
    let decision = policy_eval::evaluate(
        policy_id,
        &policy,
        &request.actions,
        EvalContext {
            root: &root,
            cwd: &cwd,
        },
    )
    .map_err(fail(&event))?;
    Ok((event, decision))
}
