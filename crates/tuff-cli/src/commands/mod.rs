mod add;
mod agent;
mod cache;
mod check_cmd;
mod create;
mod delete;
mod diff;
mod hooks;
mod init;
mod list;
mod outdated;
mod report;
mod status;
mod update;

pub use add::*;
pub use agent::*;
pub use cache::*;
pub use check_cmd::*;
pub use create::*;
pub use delete::*;
pub use diff::*;
pub use hooks::*;
pub use init::*;
pub use list::*;
pub use outdated::*;
pub use report::*;
pub use status::*;
pub use update::*;

use std::path::{Path, PathBuf};

use crate::adapter::{AdapterKind, AgentAdapter};
use crate::config;
use crate::error::{Result, TuffError};
use crate::manifest::CapabilityType;

// ── Display utilities (used by multiple submodules) ─────────────────────────

pub(crate) fn use_color() -> bool {
    std::env::var_os("NO_COLOR").is_none()
}

pub(crate) fn paint(text: &str, code: &str) -> String {
    if use_color() {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub(crate) fn style_drift_status(status: &str) -> String {
    match status {
        "clean" => format!("{} {}", paint("✓", "32"), paint("clean", "32")),
        "modified" => format!("{} {}", paint("●", "33"), paint("modified", "33")),
        "missing" => format!("{} {}", paint("✗", "31"), paint("missing", "31")),
        "error" => format!("{} {}", paint("✗", "31"), paint("error", "31")),
        other => other.to_string(),
    }
}

pub(crate) fn style_capability_type(capability_type: CapabilityType) -> String {
    let s = capability_type.as_str();
    let code = match capability_type {
        CapabilityType::Skill => "36",
        CapabilityType::Tool => "35",
        CapabilityType::Hook => "33",
        CapabilityType::Workflow => "34",
        CapabilityType::Policy => "31",
    };
    paint(s, code)
}

pub(crate) fn style_outdated_status(status: &str) -> String {
    match status {
        "up to date" => format!("{} {}", paint("✓", "32"), paint("up to date", "32")),
        "outdated" => format!("{} {}", paint("●", "33"), paint("outdated", "33")),
        "error" => format!("{} {}", paint("✗", "31"), paint("error", "31")),
        other => other.to_string(),
    }
}

pub(crate) fn visible_width(text: &str) -> usize {
    let mut width = 0usize;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                let _ = chars.next();
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        width += 1;
    }

    width
}

fn pad_cell(text: &str, width: usize) -> String {
    let padding = width.saturating_sub(visible_width(text));
    format!("{text}{}", " ".repeat(padding))
}

fn border(left: char, join: char, right: char, widths: &[usize]) -> String {
    let mut line = String::new();
    line.push(left);
    for (index, width) in widths.iter().enumerate() {
        line.push_str(&"─".repeat(width + 2));
        if index + 1 == widths.len() {
            line.push(right);
        } else {
            line.push(join);
        }
    }
    line
}

pub(crate) fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|header| header.len()).collect();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(visible_width(cell));
        }
    }

    let mut out = String::new();
    out.push_str(&border('┌', '┬', '┐', &widths));
    out.push('\n');

    out.push('│');
    for (index, header) in headers.iter().enumerate() {
        out.push(' ');
        out.push_str(&pad_cell(header, widths[index]));
        out.push(' ');
        out.push('│');
    }
    out.push('\n');

    out.push_str(&border('├', '┼', '┤', &widths));
    out.push('\n');

    for row in rows {
        out.push('│');
        for (index, cell) in row.iter().enumerate() {
            out.push(' ');
            out.push_str(&pad_cell(cell, widths[index]));
            out.push(' ');
            out.push('│');
        }
        out.push('\n');
    }

    out.push_str(&border('└', '┴', '┘', &widths));
    out
}

// ── Shared utility functions ────────────────────────────────────────────────

pub(crate) fn short_sha(v: &str) -> &str {
    if v.len() >= 7 && v.chars().all(|c| c.is_ascii_hexdigit()) {
        &v[..7]
    } else {
        v
    }
}

pub(crate) fn markdown_escape(value: &str) -> String {
    value.replace('|', "\\|")
}

pub(crate) fn home_dir() -> Result<PathBuf> {
    home_dir_opt().ok_or_else(|| TuffError::new("HOME environment variable not set"))
}

pub(crate) fn home_dir_opt() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

pub(crate) fn resolve_agent_selection(
    root: &Path,
    requested: &[String],
    global: bool,
) -> Result<Vec<String>> {
    let values = if requested.is_empty() {
        let config = if global {
            config::read_global_config(root)?
        } else {
            config::read_config(root)?
        };
        vec![config.default_agent]
    } else {
        requested.to_vec()
    };

    let mut selected = Vec::new();
    for value in values {
        let adapter = AdapterKind::from_id(&value).ok_or_else(|| {
            TuffError::new(format!(
                "unknown agent '{}'; use 'tuff agent list' to see available agents",
                value
            ))
        })?;
        if !selected.iter().any(|existing| existing == adapter.id()) {
            selected.push(adapter.id().to_string());
        }
    }
    Ok(selected)
}

pub(crate) fn infer_from_path(path: &Path) -> (CapabilityType, String) {
    let parent = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let ctype = match parent.as_str() {
        "tools" => CapabilityType::Tool,
        "hooks" => CapabilityType::Hook,
        "workflows" => CapabilityType::Workflow,
        _ => CapabilityType::Skill,
    };

    let grandparent = path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let target = match grandparent.as_str() {
        ".claude" => "claude",
        _ => "open-agents",
    };

    (ctype, target.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_sha_truncates_hex() {
        let sha = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0";
        assert_eq!(short_sha(sha), "a1b2c3d");
    }

    #[test]
    fn short_sha_keeps_short_strings() {
        assert_eq!(short_sha("abc"), "abc");
        assert_eq!(short_sha("1.0.0"), "1.0.0");
    }

    #[test]
    fn short_sha_handles_non_hex_long_string() {
        assert_eq!(short_sha("abcdefgh1234567890"), "abcdefgh1234567890");
    }
}
