use std::path::Path;

use diffy::merge;
use similar::{ChangeTag, TextDiff};

use crate::error::{CoralError, Result};

pub struct ConflictReport {
    pub file_path: String,
    pub conflicts: Vec<ConflictBlock>,
}

pub struct ConflictBlock {
    pub description: String,
    pub local: String,
    pub upstream: String,
}

#[allow(dead_code)]
pub fn diff_baseline_vs_local(
    baseline_path: &Path,
    local_path: &Path,
) -> Result<String> {
    let baseline = std::fs::read_to_string(baseline_path)?;
    let local = std::fs::read_to_string(local_path)?;

    if baseline == local {
        return Ok(String::new());
    }

    let diff = TextDiff::from_lines(&baseline, &local);
    let mut output = format!(
        "--- baseline/{}\n+++ {}\n",
        baseline_path.file_name().unwrap_or_default().to_string_lossy(),
        local_path.display()
    );
    for group in diff.grouped_ops(3) {
        for operation in group {
            for change in diff.iter_changes(&operation) {
                let sign = match change.tag() {
                    ChangeTag::Delete => "-",
                    ChangeTag::Insert => "+",
                    ChangeTag::Equal => " ",
                };
                output.push_str(sign);
                output.push_str(change.value());
            }
        }
    }
    Ok(output)
}

#[allow(dead_code)]
pub fn three_way_merge(
    file_name: &str,
    baseline: &str,
    local: &str,
    upstream: &str,
) -> Option<ConflictReport> {
    if local == upstream {
        return None;
    }

    match merge(baseline, local, upstream) {
        Ok(_merged) => None,
        Err(conflict_text) => {
            // Extract conflict blocks from the merge output
            let mut blocks = Vec::new();
            let mut in_conflict = false;
            let mut local_lines = Vec::new();
            let mut upstream_lines = Vec::new();
            let mut line_start = 0;

            for (i, line) in conflict_text.lines().enumerate() {
                if line.starts_with("<<<<<<") {
                    in_conflict = true;
                    local_lines.clear();
                    upstream_lines.clear();
                    line_start = i;
                } else if line.starts_with("======") {
                    std::mem::swap(&mut local_lines, &mut upstream_lines);
                } else if line.starts_with(">>>>>>") {
                    in_conflict = false;
                    blocks.push(ConflictBlock {
                        description: format!(
                            "Conflict in \"{}\" (lines {}-{})",
                            file_name,
                            line_start + 1,
                            i + 1
                        ),
                        local: local_lines.join("\n"),
                        upstream: upstream_lines.join("\n"),
                    });
                    local_lines.clear();
                    upstream_lines.clear();
                } else if in_conflict {
                    if upstream_lines.is_empty() {
                        local_lines.push(line.to_string());
                    } else {
                        upstream_lines.push(line.to_string());
                    }
                }
            }
            Some(ConflictReport {
                file_path: file_name.to_string(),
                conflicts: blocks,
            })
        }
    }
}

pub fn merge_and_write(
    baseline_path: &Path,
    local_path: &Path,
    upstream_path: &Path,
) -> Result<Option<Vec<ConflictReport>>> {
    let mut reports = Vec::new();

    let baseline = std::fs::read_to_string(baseline_path)?;
    let local = std::fs::read_to_string(local_path)?;
    let upstream = std::fs::read_to_string(upstream_path)?;

    if local == upstream {
        return Ok(None);
    }

    if local == baseline {
        // clean local, upstream changed → apply upstream
        std::fs::write(local_path, &upstream)?;
        return Ok(None);
    }

    if upstream == baseline {
        // modified local, upstream unchanged → no-op
        return Ok(None);
    }

    // modified local, changed upstream → attempt merge
    let file_name = local_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();

    match merge(&baseline, &local, &upstream) {
        Ok(merged) => {
            // clean merge — write merged result
            std::fs::write(local_path, &merged)?;
            Ok(None)
        }
        Err(conflict_text) => {
            let mut blocks = Vec::new();
            let mut in_conflict = false;
            let mut local_lines: Vec<String> = Vec::new();
            let mut upstream_lines: Vec<String> = Vec::new();
            let mut line_start = 0;

            for (i, line) in conflict_text.lines().enumerate() {
                if line.starts_with("<<<<<<") {
                    local_lines.clear();
                    upstream_lines.clear();
                    line_start = i;
                    in_conflict = true;
                } else if in_conflict && line.starts_with("======") {
                    std::mem::swap(&mut local_lines, &mut upstream_lines);
                } else if in_conflict && line.starts_with(">>>>>>") {
                    in_conflict = false;
                    blocks.push(ConflictBlock {
                        description: format!(
                            "Conflict in \"{}\" (lines {}-{})",
                            file_name,
                            line_start + 1,
                            i + 1
                        ),
                        local: local_lines.join("\n"),
                        upstream: upstream_lines.join("\n"),
                    });
                    local_lines.clear();
                    upstream_lines.clear();
                } else if in_conflict {
                    local_lines.push(line.to_string());
                }
            }
            reports.push(ConflictReport {
                file_path: file_name.to_string(),
                conflicts: blocks,
            });
            Ok(Some(reports))
        }
    }
}

pub fn get_upstream_content(
    cache_dir: &Path,
    skill_name: &str,
    file_rel_path: &str,
) -> Result<String> {
    let skill_dir = crate::git::discover_skill(cache_dir, skill_name)?;
    let full_path = skill_dir.join(file_rel_path);
    if !full_path.exists() {
        return Err(CoralError::new(format!(
            "upstream file not found: {}",
            full_path.display()
        )));
    }
    Ok(std::fs::read_to_string(full_path)?)
}
