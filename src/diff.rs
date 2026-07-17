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
pub fn diff_baseline_vs_local(baseline_path: &Path, local_path: &Path) -> Result<String> {
    let baseline = std::fs::read_to_string(baseline_path)?;
    let local = std::fs::read_to_string(local_path)?;

    if baseline == local {
        return Ok(String::new());
    }

    let diff = TextDiff::from_lines(&baseline, &local);
    let mut output = format!(
        "--- baseline/{}\n+++ {}\n",
        baseline_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
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
    let file_name = local_path.file_name().unwrap_or_default().to_string_lossy();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_clean_local_upstream_changed_returns_none() {
        assert!(three_way_merge("file.md", "original", "original", "modified upstream").is_none());
    }

    #[test]
    fn merge_local_unchanged_returns_none_when_same() {
        assert!(three_way_merge("file.md", "original", "original", "original").is_none());
    }

    #[test]
    fn merge_conflict_both_changed_reports_conflicts() {
        let report = three_way_merge("file.md", "original", "local change", "upstream change");
        assert!(report.is_some());
        assert_eq!(report.unwrap().file_path, "file.md");
    }

    #[test]
    fn merge_non_conflicting_changes_succeeds() {
        let report = three_way_merge(
            "file.md",
            "line1\nline2\nline3\nline4",
            "line1\nline2 local\nline3\nline4",
            "line1\nline2\nline3\nline4 upstream",
        );
        assert!(report.is_none());
    }

    #[test]
    fn diff_baseline_vs_local_shows_changes() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path().join("base.md");
        let local = tmp.path().join("local.md");
        std::fs::write(&base, "hello world").unwrap();
        std::fs::write(&local, "hello modified").unwrap();
        let diff = diff_baseline_vs_local(&base, &local).unwrap();
        assert!(diff.contains("-hello world"));
        assert!(diff.contains("+hello modified"));
    }

    #[test]
    fn diff_identical_files_returns_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path().join("base.md");
        std::fs::write(&base, "same").unwrap();
        let local = tmp.path().join("local.md");
        std::fs::write(&local, "same").unwrap();
        assert!(diff_baseline_vs_local(&base, &local).unwrap().is_empty());
    }
}

#[test]
fn merge_and_write_clean_local_upstream_changed_applies_upstream() {
    let tmp = tempfile::TempDir::new().unwrap();
    let base = tmp.path().join("base.md");
    let local = tmp.path().join("local.md");
    let upstream = tmp.path().join("upstream.md");
    std::fs::write(&base, "original").unwrap();
    std::fs::write(&local, "original").unwrap();
    std::fs::write(&upstream, "new upstream version").unwrap();

    let result = merge_and_write(&base, &local, &upstream).unwrap();
    assert!(result.is_none());
    assert_eq!(
        std::fs::read_to_string(&local).unwrap(),
        "new upstream version"
    );
}

#[test]
fn merge_and_write_local_modified_upstream_unchanged_no_op() {
    let tmp = tempfile::TempDir::new().unwrap();
    let base = tmp.path().join("base.md");
    let local = tmp.path().join("local.md");
    let upstream = tmp.path().join("upstream.md");
    std::fs::write(&base, "original").unwrap();
    std::fs::write(&local, "local modified").unwrap();
    std::fs::write(&upstream, "original").unwrap();

    let result = merge_and_write(&base, &local, &upstream).unwrap();
    assert!(result.is_none());
    assert_eq!(std::fs::read_to_string(&local).unwrap(), "local modified");
}

#[test]
fn merge_and_write_local_and_upstream_identical_no_op() {
    let tmp = tempfile::TempDir::new().unwrap();
    let base = tmp.path().join("base.md");
    let local = tmp.path().join("local.md");
    let upstream = tmp.path().join("upstream.md");
    std::fs::write(&base, "original").unwrap();
    std::fs::write(&local, "changed").unwrap();
    std::fs::write(&upstream, "changed").unwrap();

    let result = merge_and_write(&base, &local, &upstream).unwrap();
    assert!(result.is_none());
    assert_eq!(std::fs::read_to_string(&local).unwrap(), "changed");
}

#[test]
fn merge_and_write_conflicting_changes_reports_conflict() {
    let tmp = tempfile::TempDir::new().unwrap();
    let base = tmp.path().join("base.md");
    let local = tmp.path().join("local.md");
    let upstream = tmp.path().join("upstream.md");
    std::fs::write(&base, "original line\nshared line").unwrap();
    std::fs::write(&local, "local edit\nshared line").unwrap();
    std::fs::write(&upstream, "upstream edit\nshared line").unwrap();

    let result = merge_and_write(&base, &local, &upstream).unwrap();
    assert!(result.is_some());
    // File should NOT be overwritten on conflict
    assert_eq!(
        std::fs::read_to_string(&local).unwrap(),
        "local edit\nshared line"
    );
}
