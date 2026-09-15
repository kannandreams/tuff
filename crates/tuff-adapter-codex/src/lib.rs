use std::path::Path;

use tuff_hooks_spec::{
    CompatibilityEntry, CompatibilityMatrix, CoverageLevel, HookEvent, SPEC_VERSION,
};

use tuff_core::adapter::{AgentAdapter, HookSettingsShape};
use tuff_core::error::Result;
use tuff_core::manifest::CapabilityType;
use tuff_core::policy::{
    PolicyCoverageEntry, PolicyEffect, PolicyRule, PolicySubject, PolicySubjectKind,
};

pub const ID: &str = "codex";
pub const DISPLAY_NAME: &str = "Codex";
pub const SUPPORTED_TYPES: &[CapabilityType] = &[
    CapabilityType::Skill,
    CapabilityType::Tool,
    CapabilityType::Hook,
    CapabilityType::Workflow,
    CapabilityType::McpServer,
    CapabilityType::Policy,
];

pub const SUPPORTED_AGENTS: &[&str] = &["Codex"];

pub const HOOK_SETTINGS_RELPATH: &str = ".agents/hook.json";

/// The rules file Tuff owns for compiled policy command rules. Codex loads
/// every `.rules` file under `<repo>/.codex/rules/` in a trusted project.
pub const RULES_RELPATH: &str = ".codex/rules/tuff.rules";
const CODEX_RULES_DOCS: &str = "https://developers.openai.com/codex/rules";

/// How Codex enforces each kind of policy rule, from its rules documentation
/// and checked against Codex CLI 0.154.0 on 2026-09-15. Command rules
/// compile to `prefix_rule` entries; Codex rules match commands only.
pub fn policy_matrix() -> Vec<PolicyCoverageEntry> {
    const COMMAND: &str = "matches the command's leading words, and each command of a simple chain joined by &&, ||, ; or |; a script with redirection, $(...), a variable assignment, a wildcard, or control flow is matched as one command and not caught, and a program run by absolute path such as /usr/bin/git may not be matched; Codex loads project rules only in a trusted project and labels rules experimental";
    const FILES: &str = "Codex rules match commands, not file paths, and Tuff does not compile Codex's sandbox permission profiles";
    const MCP: &str = "Tuff does not compile MCP tool rules for Codex yet";
    let row =
        |effect, subject, coverage, mechanism: Option<&str>, caveat: String| PolicyCoverageEntry {
            effect,
            subject,
            coverage,
            mechanism: mechanism.map(str::to_string),
            caveat: Some(caveat),
            source: Some(CODEX_RULES_DOCS.to_string()),
        };
    use PolicyEffect::{Ask, Deny};
    use PolicySubjectKind::{Command, Edit, Mcp, Read};
    use tuff_hooks_spec::CoverageLevel::{Partial, Unsupported};
    vec![
        row(
            Deny,
            Command,
            Partial,
            Some(".codex/rules/tuff.rules prefix_rule(decision = \"forbidden\")"),
            COMMAND.to_string(),
        ),
        row(Deny, Read, Unsupported, None, FILES.to_string()),
        row(Deny, Edit, Unsupported, None, FILES.to_string()),
        row(Deny, Mcp, Unsupported, None, MCP.to_string()),
        row(
            Ask,
            Command,
            Partial,
            Some(".codex/rules/tuff.rules prefix_rule(decision = \"prompt\")"),
            format!(
                "{COMMAND}; where Codex never asks for approval, as in codex exec by default, the command is refused"
            ),
        ),
        row(Ask, Read, Unsupported, None, FILES.to_string()),
        row(Ask, Edit, Unsupported, None, FILES.to_string()),
        row(Ask, Mcp, Unsupported, None, MCP.to_string()),
    ]
}

/// The Codex rules file entry one policy rule compiles to, or `None` for a
/// kind of rule Codex rules cannot express.
///
/// `deny` becomes `decision = "forbidden"` and `ask` becomes
/// `decision = "prompt"`. The rule's `reason`, when given, becomes the
/// `justification` Codex shows when it refuses the command.
pub fn permission_rules(rule: &PolicyRule) -> Result<Option<Vec<String>>> {
    let PolicySubject::Command(arguments) = rule.subject()? else {
        return Ok(None);
    };
    let decision = match rule.effect()? {
        PolicyEffect::Deny => "forbidden",
        PolicyEffect::Ask => "prompt",
    };
    let pattern = arguments
        .iter()
        .map(|argument| starlark_string(argument))
        .collect::<Vec<_>>()
        .join(", ");
    let justification = rule
        .reason
        .as_deref()
        .map(|reason| format!(", justification = {}", starlark_string(reason)))
        .unwrap_or_default();
    Ok(Some(vec![format!(
        "prefix_rule(pattern = [{pattern}], decision = \"{decision}\"{justification})"
    )]))
}

/// A double-quoted Starlark string literal. A policy has already refused
/// whitespace in command arguments, so control characters can only come
/// from a reason, where a space keeps the rule on one line.
fn starlark_string(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for character in text.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            character if character.is_control() => quoted.push(' '),
            character => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

pub struct Codex;

pub const HOOK_COMPATIBILITY: CompatibilityMatrix = CompatibilityMatrix {
    spec_version: SPEC_VERSION,
    adapter: ID,
    events: &[
        CompatibilityEntry {
            event: HookEvent::BeforeFinish,
            native_event: Some("before_finish"),
            aliases: &[],
            coverage: CoverageLevel::Full,
            scope: &[],
            caveat: None,
            source: None,
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::AfterSave,
            native_event: Some("after_save"),
            aliases: &[],
            coverage: CoverageLevel::Full,
            scope: &[],
            caveat: None,
            source: None,
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::PreToolUse,
            native_event: Some("pre_tool_execution"),
            aliases: &["pre_tool_execution"],
            coverage: CoverageLevel::Partial,
            scope: &["local function tools", "Bash", "Edit", "Write", "MCP"],
            caveat: Some("Codex hosted tools do not use the local function-tool hook path."),
            source: Some("https://learn.chatgpt.com/docs/hooks.md"),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::PostToolUse,
            native_event: Some("post_tool_execution"),
            aliases: &["post_tool_execution"],
            coverage: CoverageLevel::Partial,
            scope: &["local function tools", "Bash", "Edit", "Write", "MCP"],
            caveat: Some("Codex hosted tools do not use the local function-tool hook path."),
            source: Some("https://learn.chatgpt.com/docs/hooks.md"),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::SessionStart,
            native_event: None,
            aliases: &[],
            coverage: CoverageLevel::Unsupported,
            scope: &[],
            caveat: Some("Codex hook.json does not currently define a session-start event."),
            source: None,
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::SessionEnd,
            native_event: None,
            aliases: &[],
            coverage: CoverageLevel::Unsupported,
            scope: &[],
            caveat: Some("Codex hook.json does not currently define a session-end event."),
            source: None,
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::Stop,
            native_event: None,
            aliases: &[],
            coverage: CoverageLevel::Unsupported,
            scope: &[],
            caveat: Some("Codex hook.json does not currently define a stop event."),
            source: None,
            since_harness_version: None,
            until_harness_version: None,
        },
    ],
};

impl AgentAdapter for Codex {
    fn id(&self) -> &'static str {
        ID
    }

    fn display_name(&self) -> &'static str {
        DISPLAY_NAME
    }

    fn dir_prefix(&self) -> &'static str {
        ".agents"
    }

    fn mcp_config_relpath(&self) -> &'static str {
        ".agents/mcp.json"
    }

    fn supported_agents(&self) -> &[&'static str] {
        SUPPORTED_AGENTS
    }

    fn kinds_supported(&self) -> &[CapabilityType] {
        SUPPORTED_TYPES
    }

    fn hook_compatibility(&self) -> &'static CompatibilityMatrix {
        &HOOK_COMPATIBILITY
    }

    fn hook_settings_relpath(&self) -> &'static str {
        HOOK_SETTINGS_RELPATH
    }

    fn scaffold_hook_event(&self) -> &'static str {
        "before_finish"
    }

    fn hook_settings_shape(&self) -> HookSettingsShape {
        HookSettingsShape::Grouped
    }

    fn policy_compatibility(&self) -> Vec<PolicyCoverageEntry> {
        policy_matrix()
    }

    fn permissions_settings_relpath(&self) -> Option<&'static str> {
        Some(RULES_RELPATH)
    }

    fn native_permission_rules(&self, rule: &PolicyRule) -> Result<Option<Vec<String>>> {
        permission_rules(rule)
    }

    fn detect(&self, repo_root: &Path) -> bool {
        repo_root.join(".agents").exists() || repo_root.join("AGENTS.md").exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC-106 D2: Codex shares Claude Code's remote-server shape, `type`
    /// and `${VAR}` both. Pinned per adapter rather than inferred from the
    /// shared default, because assuming harnesses agree is what produced
    /// debt item #1.
    #[test]
    fn a_remote_server_entry_declares_type_and_renders_headers() {
        let server = tuff_core::manifest::McpServerConfig {
            transport: tuff_core::manifest::McpTransport::Http,
            command: None,
            args: Vec::new(),
            url: Some("https://mcp.example.test/mcp".to_string()),
            env: Default::default(),
            headers: [(
                "Authorization".to_string(),
                tuff_core::manifest::HeaderRef {
                    from_env: "EXAMPLE_TOKEN".to_string(),
                    format: Some("Bearer {}".to_string()),
                },
            )]
            .into_iter()
            .collect(),
            metadata: None,
        };

        let entry = Codex.mcp_server_entry(&server);

        assert_eq!(
            entry,
            serde_json::json!({
                "type": "http",
                "url": "https://mcp.example.test/mcp",
                "headers": {"Authorization": "Bearer ${EXAMPLE_TOKEN}"},
            })
        );
    }

    #[test]
    fn command_rules_compile_to_prefix_rules_and_other_subjects_to_nothing() {
        let rule = |effect: &str| PolicyRule {
            effect: effect.to_string(),
            command: None,
            read: None,
            edit: None,
            mcp: None,
            reason: None,
        };
        let words = |words: &[&str]| Some(words.iter().map(|word| word.to_string()).collect());
        let rules = [
            PolicyRule {
                command: words(&["git", "push", "--force"]),
                reason: Some("Force pushes rewrite \"shared\" history.".to_string()),
                ..rule("deny")
            },
            PolicyRule {
                command: words(&["terraform", "apply"]),
                ..rule("ask")
            },
            PolicyRule {
                read: words(&[".env"]),
                ..rule("deny")
            },
            PolicyRule {
                mcp: Some("github:delete_*".to_string()),
                ..rule("deny")
            },
        ];
        let compiled: Vec<_> = rules
            .iter()
            .map(|rule| permission_rules(rule).unwrap())
            .collect();
        assert_eq!(
            compiled,
            vec![
                Some(vec![
                    r#"prefix_rule(pattern = ["git", "push", "--force"], decision = "forbidden", justification = "Force pushes rewrite \"shared\" history.")"#
                        .to_string()
                ]),
                Some(vec![
                    r#"prefix_rule(pattern = ["terraform", "apply"], decision = "prompt")"#
                        .to_string()
                ]),
                None,
                None,
            ]
        );
    }

    #[test]
    fn the_policy_matrix_enforces_command_rules_only() {
        let matrix = Codex.policy_compatibility();
        assert_eq!(matrix.len(), 8);
        for entry in &matrix {
            let expected = if entry.subject == PolicySubjectKind::Command {
                tuff_hooks_spec::CoverageLevel::Partial
            } else {
                tuff_hooks_spec::CoverageLevel::Unsupported
            };
            assert_eq!(entry.coverage, expected, "{entry:?}");
            assert!(entry.caveat.is_some(), "{entry:?}");
        }
    }

    #[test]
    fn id_and_display_name_are_not_empty() {
        assert!(!ID.is_empty());
        assert!(!DISPLAY_NAME.is_empty());
    }

    #[test]
    fn supported_types_covers_all_capability_types() {
        assert_eq!(SUPPORTED_TYPES.len(), 6);
    }

    #[test]
    fn merging_the_same_fragment_twice_does_not_duplicate_the_hook() {
        let fragment = serde_json::json!({
            "hooks": {
                "before_finish": [{"hooks": [{"type": "command", "command": "sh .agents/hooks/demo/run.sh"}]}]
            }
        });

        let once = Codex
            .merge_hook_fragment(None, &fragment)
            .expect("first merge");
        let twice = Codex
            .merge_hook_fragment(Some(&once), &fragment)
            .expect("second merge");

        let settings: serde_json::Value = serde_json::from_slice(&twice).expect("valid json");
        let groups = settings["hooks"]["before_finish"]
            .as_array()
            .expect("event array");
        assert_eq!(
            groups.len(),
            1,
            "re-adding a hook must not register it twice"
        );
        assert_eq!(
            once, twice,
            "a redundant merge must leave the file unchanged"
        );
    }
}
