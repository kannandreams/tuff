use std::path::Path;

use crate::manifest::CapabilityType;

pub const ID: &str = "claude";
pub const DISPLAY_NAME: &str = "Claude";
pub const SUPPORTED_TYPES: &[CapabilityType] = &[
    CapabilityType::Skill,
    CapabilityType::Tool,
    CapabilityType::Hook,
    CapabilityType::Workflow,
];

pub const SUPPORTED_AGENTS: &[&str] = &["Claude Code"];

pub const SUPPORTED_EVENTS: &[&str] = &["before_finish", "post_tool_execution"];

pub fn detect(repo_root: &Path) -> bool {
    repo_root.join(".claude").exists() || repo_root.join("CLAUDE.md").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_not_empty() {
        assert!(!ID.is_empty());
        assert!(!DISPLAY_NAME.is_empty());
        assert!(!SUPPORTED_EVENTS.is_empty());
    }

    #[test]
    fn supported_types_covers_all_capability_types() {
        assert_eq!(SUPPORTED_TYPES.len(), 4);
    }
}
