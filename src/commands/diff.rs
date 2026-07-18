use std::path::{Path, PathBuf};

use crate::error::{CoralError, Result};
use crate::git;
use crate::lockfile;
use crate::resolver::{self, Scope};

use super::{capability_relative_path, home_dir, paint, resolve_agent_selection};

pub fn cmd_diff(
    repo_root: &Path,
    capability_id: &str,
    target: Option<&str>,
    upstream: bool,
) -> Result<()> {
    let (scope, entry, scope_root) = match resolver::resolve_entry(capability_id, repo_root)? {
        Some((s, e)) => {
            let root = match s {
                Scope::Project => repo_root.to_path_buf(),
                Scope::Global => home_dir()?,
            };
            (s, e, root)
        }
        None => {
            return Err(CoralError::new(format!(
                "capability is not installed: {capability_id}"
            )));
        }
    };

    let _ = scope;

    let requested = target
        .map(|value| vec![value.to_string()])
        .unwrap_or_default();
    let selected_targets = resolve_agent_selection(&scope_root, &requested)?;

    if upstream {
        return cmd_diff_upstream(
            scope_root,
            capability_id,
            &entry,
            selected_targets.first().map(String::as_str),
        );
    }

    let mut output = String::new();

    if selected_targets.len() == 1 {
        let tid = &selected_targets[0];
        output.push_str(&lockfile::diff_against_baseline(
            &scope_root,
            capability_id,
            tid,
            &entry,
        )?);
    } else {
        let mut first = true;
        for tid in &selected_targets {
            let diff =
                lockfile::diff_against_baseline(&scope_root, capability_id, tid, &entry)?;
            if diff.is_empty() {
                continue;
            }
            if !first {
                output.push('\n');
            }
            output.push_str(&diff);
            first = false;
        }
    }

    print!("{}", style_diff(&output));
    Ok(())
}

fn cmd_diff_upstream(
    scope_root: PathBuf,
    _capability_id: &str,
    entry: &lockfile::CapabilityLockEntry,
    target: Option<&str>,
) -> Result<()> {
    let source = entry.source.as_ref().ok_or_else(|| {
        CoralError::new("upstream diff only available for git-sourced primitives")
    })?;

    let (cache_dir, _) = git::clone_or_fetch(&source.url)?;
    let mut output = String::new();

    for (tid, target_entry) in &entry.targets {
        if let Some(t) = target
            && tid != t {
                continue;
            }

        for emitted in &target_entry.emitted_files {
            let rel_path = capability_relative_path(&emitted.path, &source.skill);
            let rel_display = rel_path.to_string_lossy();

            let upstream_content =
                crate::diff::get_upstream_content(&cache_dir, &source.skill, &rel_display)?;
            let baseline_content = String::from_utf8(lockfile::read_baseline_object(
                &scope_root,
                &emitted.baseline_hash,
            )?)
            .map_err(|error| {
                CoralError::new(format!(
                    "baseline object is not valid UTF-8 for '{}': {}",
                    emitted.path, error
                ))
            })?;

            if baseline_content == upstream_content {
                continue;
            }

            output.push_str(&format!(
                "--- baseline/{}\n+++ upstream/{}/{}\n",
                rel_display, tid, rel_display
            ));
            let diff = similar::TextDiff::from_lines(&baseline_content, &upstream_content);
            for group in diff.grouped_ops(3) {
                for operation in group {
                    for change in diff.iter_changes(&operation) {
                        let sign = match change.tag() {
                            similar::ChangeTag::Delete => "-",
                            similar::ChangeTag::Insert => "+",
                            similar::ChangeTag::Equal => " ",
                        };
                        output.push_str(sign);
                        output.push_str(change.value());
                    }
                }
            }
        }
    }

    if output.is_empty() {
        println!("no upstream changes");
    } else {
        print!("{}", style_diff(&output));
    }

    Ok(())
}

fn style_diff(diff: &str) -> String {
    diff.split_inclusive('\n')
        .map(|line| {
            let code = if line.starts_with("+++") || line.starts_with("---") {
                "36"
            } else if line.starts_with('+') {
                "32"
            } else if line.starts_with('-') {
                "31"
            } else {
                return line.to_string();
            };
            paint(line, code)
        })
        .collect()
}
