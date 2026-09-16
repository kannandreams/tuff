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

pub const ID: &str = "claude";
pub const DISPLAY_NAME: &str = "Claude";
pub const SETTINGS_RELPATH: &str = ".claude/settings.json";
pub struct Claude;
pub const SUPPORTED_TYPES: &[CapabilityType] = &[
    CapabilityType::Skill,
    CapabilityType::Tool,
    CapabilityType::Hook,
    CapabilityType::McpServer,
    CapabilityType::Policy,
];

pub const SUPPORTED_AGENTS: &[&str] = &["Claude Code"];
const CLAUDE_PERMISSIONS_DOCS: &str = "https://code.claude.com/docs/en/permissions";

/// How Claude Code enforces each kind of policy rule, from its permissions
/// documentation. Command and file rules are real but not boundaries, and
/// the caveats say how; an MCP rule names the tool itself.
pub fn policy_matrix() -> Vec<PolicyCoverageEntry> {
    const COMMAND: &str = "matches the command as Claude writes it, including inside compound commands; the same program run another way, such as by absolute path, through sh -c, or as git -C . push, is not matched";
    const FILES: &str = "covers Claude's file tools and the shell commands Claude Code recognises, such as cat and sed, not a script or program that opens the file itself";
    let row =
        |effect, subject, coverage, mechanism: &str, caveat: Option<&str>| PolicyCoverageEntry {
            effect,
            subject,
            coverage,
            mechanism: Some(mechanism.to_string()),
            caveat: caveat.map(str::to_string),
            source: Some(CLAUDE_PERMISSIONS_DOCS.to_string()),
        };
    use PolicyEffect::{Ask, Deny};
    use PolicySubjectKind::{Command, Edit, Mcp, Read};
    vec![
        row(
            Deny,
            Command,
            CoverageLevel::Partial,
            "permissions.deny Bash(<command> *)",
            Some(COMMAND),
        ),
        row(
            Deny,
            Read,
            CoverageLevel::Partial,
            "permissions.deny Read(<path>)",
            Some(FILES),
        ),
        row(
            Deny,
            Edit,
            CoverageLevel::Partial,
            "permissions.deny Edit(<path>)",
            Some(FILES),
        ),
        row(
            Deny,
            Mcp,
            CoverageLevel::Full,
            "permissions.deny mcp__<server>__<tool>",
            None,
        ),
        row(
            Ask,
            Command,
            CoverageLevel::Partial,
            "permissions.ask Bash(<command> *)",
            Some(COMMAND),
        ),
        row(
            Ask,
            Read,
            CoverageLevel::Partial,
            "permissions.ask Read(<path>)",
            Some(FILES),
        ),
        row(
            Ask,
            Edit,
            CoverageLevel::Partial,
            "permissions.ask Edit(<path>)",
            Some(FILES),
        ),
        row(
            Ask,
            Mcp,
            CoverageLevel::Full,
            "permissions.ask mcp__<server>__<tool>",
            None,
        ),
    ]
}

/// A policy path pattern, read as `.gitignore` reads a pattern in a file at
/// the project root, as a Claude Code path rule anchored at the project.
///
/// A pattern with a `/` at its start or middle is anchored where it is, so
/// `secrets/**` becomes `/secrets/**`. One without is matched at any depth,
/// so `.env` becomes `/**/.env`. A trailing `/` names a directory's
/// contents. The leading `/` anchors at the project in Claude Code's project
/// settings, where `//` would mean the filesystem root.
pub fn permission_path(pattern: &str) -> String {
    let pattern = pattern.trim_start_matches("./");
    let anchored = pattern.trim_end_matches('/').contains('/');
    let normalized = if pattern.ends_with('/') {
        format!("{pattern}**")
    } else {
        pattern.to_string()
    };
    if anchored {
        format!("/{normalized}")
    } else {
        format!("/**/{normalized}")
    }
}

/// The Claude Code permission rules one policy rule compiles to.
pub fn permission_rules(rule: &PolicyRule) -> Result<Vec<String>> {
    Ok(match rule.subject()? {
        PolicySubject::Command(arguments) => vec![format!("Bash({} *)", arguments.join(" "))],
        PolicySubject::Read(patterns) => patterns
            .iter()
            .map(|pattern| format!("Read({})", permission_path(pattern)))
            .collect(),
        PolicySubject::Edit(patterns) => patterns
            .iter()
            .map(|pattern| format!("Edit({})", permission_path(pattern)))
            .collect(),
        PolicySubject::Mcp { server, tool } => vec![format!("mcp__{server}__{tool}")],
    })
}
const CLAUDE_HOOKS_DOCS: &str = "https://code.claude.com/docs/en/hooks";

pub const HOOK_COMPATIBILITY: CompatibilityMatrix = CompatibilityMatrix {
    spec_version: SPEC_VERSION,
    adapter: ID,
    events: &[
        CompatibilityEntry {
            event: HookEvent::SessionStart,
            native_event: Some("SessionStart"),
            aliases: &["SessionStart"],
            coverage: CoverageLevel::Full,
            scope: &["startup", "resume", "clear", "compact", "fork"],
            caveat: None,
            source: Some(CLAUDE_HOOKS_DOCS),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::SessionEnd,
            native_event: Some("SessionEnd"),
            aliases: &["SessionEnd"],
            coverage: CoverageLevel::Full,
            scope: &["session lifecycle"],
            caveat: None,
            source: Some(CLAUDE_HOOKS_DOCS),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::PreToolUse,
            native_event: Some("PreToolUse"),
            aliases: &["PreToolUse"],
            coverage: CoverageLevel::Full,
            scope: &["tool calls"],
            caveat: None,
            source: Some(CLAUDE_HOOKS_DOCS),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::PostToolUse,
            native_event: Some("PostToolUse"),
            aliases: &["PostToolUse"],
            coverage: CoverageLevel::Full,
            scope: &["successful tool calls"],
            caveat: None,
            source: Some(CLAUDE_HOOKS_DOCS),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::BeforeFinish,
            native_event: Some("Stop"),
            aliases: &[],
            coverage: CoverageLevel::Partial,
            scope: &["main-agent completion"],
            caveat: Some(
                "Claude Stop runs after the main agent finishes responding and can request continuation; it does not represent every possible pre-finish boundary.",
            ),
            source: Some(CLAUDE_HOOKS_DOCS),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::AfterSave,
            native_event: None,
            aliases: &["FileChanged"],
            coverage: CoverageLevel::Unsupported,
            scope: &[],
            caveat: Some(
                "Claude FileChanged requires watched filenames or paths that Tuff's standard after_save hook cannot currently express.",
            ),
            source: Some(CLAUDE_HOOKS_DOCS),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::Stop,
            native_event: Some("Stop"),
            aliases: &["Stop"],
            coverage: CoverageLevel::Full,
            scope: &["main-agent completion"],
            caveat: None,
            source: Some(CLAUDE_HOOKS_DOCS),
            since_harness_version: None,
            until_harness_version: None,
        },
    ],
};

impl AgentAdapter for Claude {
    fn id(&self) -> &'static str {
        ID
    }

    fn display_name(&self) -> &'static str {
        DISPLAY_NAME
    }

    fn dir_prefix(&self) -> &'static str {
        ".claude"
    }

    fn mcp_config_relpath(&self) -> &'static str {
        ".mcp.json"
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
        SETTINGS_RELPATH
    }

    fn scaffold_hook_event(&self) -> &'static str {
        "SessionStart"
    }

    fn hook_settings_shape(&self) -> HookSettingsShape {
        HookSettingsShape::Grouped
    }

    fn policy_compatibility(&self) -> Vec<PolicyCoverageEntry> {
        policy_matrix()
    }

    fn permissions_settings_relpath(&self) -> Option<&'static str> {
        Some(SETTINGS_RELPATH)
    }

    fn native_permission_rules(&self, rule: &PolicyRule) -> Result<Option<Vec<String>>> {
        permission_rules(rule).map(Some)
    }

    fn detect(&self, repo_root: &Path) -> bool {
        repo_root.join(".claude").exists() || repo_root.join("CLAUDE.md").exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC-106 D2: Claude Code keeps `"type": "http"` and expands `${VAR}`.
    #[test]
    fn a_remote_server_entry_declares_type_and_renders_headers() {
        let server = tuff_core::manifest::McpServerConfig {
            transport: tuff_core::manifest::McpTransport::Http,
            command: None,
            args: Vec::new(),
            url: Some("https://mcp.example.test/mcp".to_string()),
            env: Default::default(),
            headers: [
                (
                    "Authorization".to_string(),
                    tuff_core::manifest::HeaderRef {
                        from_env: "EXAMPLE_TOKEN".to_string(),
                        format: Some("Bearer {}".to_string()),
                    },
                ),
                (
                    "X-Api-Key".to_string(),
                    tuff_core::manifest::HeaderRef {
                        from_env: "EXAMPLE_KEY".to_string(),
                        format: None,
                    },
                ),
            ]
            .into_iter()
            .collect(),
            metadata: None,
        };

        let entry = Claude.mcp_server_entry(&server);

        assert_eq!(
            entry,
            serde_json::json!({
                "type": "http",
                "url": "https://mcp.example.test/mcp",
                "headers": {
                    "Authorization": "Bearer ${EXAMPLE_TOKEN}",
                    "X-Api-Key": "${EXAMPLE_KEY}",
                },
            })
        );
    }

    #[test]
    fn constants_are_not_empty() {
        assert!(!ID.is_empty());
        assert!(!DISPLAY_NAME.is_empty());
        assert!(!HOOK_COMPATIBILITY.events.is_empty());
    }

    #[test]
    fn supported_types_covers_all_capability_types() {
        assert_eq!(SUPPORTED_TYPES.len(), 5);
    }

    #[test]
    fn hook_matrix_matches_claude_native_event_contract() {
        let actual: Vec<_> = HOOK_COMPATIBILITY
            .events
            .iter()
            .map(|entry| {
                (
                    entry.event,
                    entry.native_event,
                    entry.aliases,
                    entry.coverage,
                )
            })
            .collect();

        assert_eq!(
            actual,
            vec![
                (
                    HookEvent::SessionStart,
                    Some("SessionStart"),
                    &["SessionStart"][..],
                    CoverageLevel::Full
                ),
                (
                    HookEvent::SessionEnd,
                    Some("SessionEnd"),
                    &["SessionEnd"][..],
                    CoverageLevel::Full
                ),
                (
                    HookEvent::PreToolUse,
                    Some("PreToolUse"),
                    &["PreToolUse"][..],
                    CoverageLevel::Full
                ),
                (
                    HookEvent::PostToolUse,
                    Some("PostToolUse"),
                    &["PostToolUse"][..],
                    CoverageLevel::Full
                ),
                (
                    HookEvent::BeforeFinish,
                    Some("Stop"),
                    &[][..],
                    CoverageLevel::Partial
                ),
                (
                    HookEvent::AfterSave,
                    None,
                    &["FileChanged"][..],
                    CoverageLevel::Unsupported
                ),
                (
                    HookEvent::Stop,
                    Some("Stop"),
                    &["Stop"][..],
                    CoverageLevel::Full
                ),
            ]
        );
    }

    #[test]
    fn merging_the_same_fragment_twice_does_not_duplicate_the_hook() {
        let fragment = serde_json::json!({
            "hooks": {
                "PreToolUse": [{"hooks": [{"type": "command", "command": "sh .claude/hooks/demo/run.sh"}]}]
            }
        });

        let once = Claude
            .merge_hook_fragment(None, &fragment)
            .expect("first merge");
        let twice = Claude
            .merge_hook_fragment(Some(&once), &fragment)
            .expect("second merge");

        let settings: serde_json::Value = serde_json::from_slice(&twice).expect("valid json");
        let groups = settings["hooks"]["PreToolUse"]
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

    fn rule(effect: &str) -> PolicyRule {
        PolicyRule {
            effect: effect.to_string(),
            command: None,
            read: None,
            edit: None,
            mcp: None,
            reason: None,
        }
    }

    fn words(items: &[&str]) -> Option<Vec<String>> {
        Some(items.iter().map(|item| item.to_string()).collect())
    }

    #[test]
    fn policy_rules_compile_to_claude_code_permission_rules() {
        let cases = [
            (
                PolicyRule {
                    command: words(&["git", "push", "--force"]),
                    ..rule("deny")
                },
                vec!["Bash(git push --force *)"],
            ),
            (
                PolicyRule {
                    command: words(&["rm"]),
                    ..rule("deny")
                },
                vec!["Bash(rm *)"],
            ),
            (
                PolicyRule {
                    read: words(&[".env", "secrets/**"]),
                    ..rule("deny")
                },
                vec!["Read(/**/.env)", "Read(/secrets/**)"],
            ),
            (
                PolicyRule {
                    read: words(&["*.pem", "./config/keys/", "certs/"]),
                    ..rule("deny")
                },
                vec![
                    "Read(/**/*.pem)",
                    "Read(/config/keys/**)",
                    "Read(/**/certs/**)",
                ],
            ),
            (
                PolicyRule {
                    edit: words(&["**/migrations/**"]),
                    ..rule("ask")
                },
                vec!["Edit(/**/migrations/**)"],
            ),
            (
                PolicyRule {
                    mcp: Some("github:delete_*".to_string()),
                    ..rule("deny")
                },
                vec!["mcp__github__delete_*"],
            ),
            (
                PolicyRule {
                    mcp: Some("*:drop_table".to_string()),
                    ..rule("deny")
                },
                vec!["mcp__*__drop_table"],
            ),
        ];
        for (policy_rule, expected) in cases {
            assert_eq!(
                permission_rules(&policy_rule).unwrap(),
                expected,
                "{policy_rule:?}"
            );
        }
    }

    #[test]
    fn claude_code_enforces_mcp_rules_fully_and_everything_else_partially() {
        let matrix = Claude.policy_compatibility();
        assert_eq!(matrix.len(), 8);
        for entry in matrix {
            let expected = if entry.subject == PolicySubjectKind::Mcp {
                CoverageLevel::Full
            } else {
                CoverageLevel::Partial
            };
            assert_eq!(entry.coverage, expected, "{entry:?}");
            assert!(
                entry.mechanism.is_some() && entry.source.is_some(),
                "{entry:?}"
            );
            assert_eq!(
                entry.caveat.is_some(),
                expected == CoverageLevel::Partial,
                "{entry:?}"
            );
        }
    }
}
