use std::path::Path;

use crate::manifest::CapabilityType;

pub const ID: &str = "open-agents";
pub const DISPLAY_NAME: &str = "Open Agents";
pub const SUPPORTED_TYPES: &[CapabilityType] = &[
    CapabilityType::Skill,
    CapabilityType::Tool,
    CapabilityType::Hook,
    CapabilityType::Workflow,
];

pub const SUPPORTED_AGENTS: &[&str] = &[
    "Codex",
    "Cursor",
    "OpenCode",
    "GitHub Copilot",
    "Gemini CLI",
    "Roo",
    "Cline",
    "Windsurf",
];

pub const SUPPORTED_EVENTS: &[&str] = &[
    "before_finish",
    "after_save",
    "pre_tool_execution",
    "post_tool_execution",
];

#[allow(dead_code)]
pub fn detect(repo_root: &Path) -> bool {
    repo_root.join(".agents").join("skills").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_and_display_name_are_not_empty() {
        assert!(!ID.is_empty());
        assert!(!DISPLAY_NAME.is_empty());
    }

    #[test]
    fn supported_types_covers_all_capability_types() {
        assert_eq!(SUPPORTED_TYPES.len(), 4);
    }
}
