use std::path::Path;

use tuff_hooks_spec::{
    CompatibilityEntry, CompatibilityMatrix, CoverageLevel, HookEvent, SPEC_VERSION,
};

use tuff_core::adapter::{AgentAdapter, HookSettingsShape};
use tuff_core::error::{Result, TuffError};
use tuff_core::manifest::{CapabilityType, McpServerConfig, McpTransport};
use tuff_core::policy::{
    PolicyCoverageEntry, PolicyEffect, PolicyRule, PolicySubject, PolicySubjectKind,
};

pub const ID: &str = "codex";
pub const DISPLAY_NAME: &str = "Codex";
pub const SUPPORTED_TYPES: &[CapabilityType] = &[
    CapabilityType::Skill,
    CapabilityType::Tool,
    CapabilityType::Hook,
    CapabilityType::McpServer,
    CapabilityType::Policy,
];

pub const SUPPORTED_AGENTS: &[&str] = &["Codex"];

/// Codex reads a project's hooks from `.codex/hooks.json`, in the grouped
/// shape Claude Code uses, and only in a trusted project after the hooks
/// are approved. Checked in Codex CLI 0.154.0 on 2026-09-16: a
/// `PreToolUse` and a `SessionStart` hook registered here both ran.
pub const HOOK_SETTINGS_RELPATH: &str = ".codex/hooks.json";
const CODEX_HOOKS_DOCS: &str = "https://learn.chatgpt.com/docs/hooks";

/// Codex reads a project's MCP servers from `.codex/config.toml`, under
/// `[mcp_servers.<id>]`, only in a trusted project. Checked in Codex CLI
/// 0.154.0 on 2026-09-16: a server declared there was listed by
/// `codex mcp list` and its tools reached a live session.
pub const MCP_CONFIG_RELPATH: &str = ".codex/config.toml";

/// The rules file Tuff owns for compiled policy command rules. Codex loads
/// every `.rules` file under `<repo>/.codex/rules/` in a trusted project.
pub const RULES_RELPATH: &str = ".codex/rules/tuff.rules";
const CODEX_RULES_DOCS: &str = "https://developers.openai.com/codex/rules";
const CODEX_MCP_DOCS: &str = "https://developers.openai.com/codex/mcp";

/// How Codex enforces each kind of policy rule, from its rules and MCP
/// documentation and checked against Codex CLI 0.154.0 on 2026-09-15 and
/// 16. Command rules compile to `prefix_rule` entries, and MCP tool rules
/// to settings on the server's table in `.codex/config.toml`.
pub fn policy_matrix() -> Vec<PolicyCoverageEntry> {
    const COMMAND: &str = "matches the command's leading words, and each command of a simple chain joined by &&, ||, ; or |; a script with redirection, $(...), a variable assignment, a wildcard, or control flow is matched as one command and not caught, and a program run by absolute path such as /usr/bin/git may not be matched; Codex loads project rules only in a trusted project and labels rules experimental";
    const FILES: &str = "Codex rules match commands, not file paths, and Tuff does not compile Codex's sandbox permission profiles";
    const MCP: &str = "the server and tool must be exact names, since Codex has no pattern form for them, and the server must be declared in .codex/config.toml, which Codex loads only in a trusted project";
    let row =
        |effect, subject, coverage, mechanism: Option<&str>, caveat: String| PolicyCoverageEntry {
            effect,
            subject,
            coverage,
            mechanism: mechanism.map(str::to_string),
            caveat: Some(caveat),
            source: Some(
                if subject == Mcp {
                    CODEX_MCP_DOCS
                } else {
                    CODEX_RULES_DOCS
                }
                .to_string(),
            ),
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
        row(
            Deny,
            Mcp,
            Partial,
            Some(".codex/config.toml [mcp_servers.<server>] disabled_tools"),
            format!("{MCP}; Codex removes the tool from the session"),
        ),
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
        row(
            Ask,
            Mcp,
            Partial,
            Some(
                ".codex/config.toml [mcp_servers.<server>.tools.<tool>] approval_mode = \"prompt\"",
            ),
            format!(
                "{MCP}; Codex approves the call without asking when its approval policy is never and the sandbox allows full disk access or is off"
            ),
        ),
    ]
}

/// The settings file a rule of one subject compiles into: command rules go
/// to the rules file, MCP tool rules to the config that declares servers.
pub fn permission_relpath(subject: PolicySubjectKind) -> Option<&'static str> {
    match subject {
        PolicySubjectKind::Command => Some(RULES_RELPATH),
        PolicySubjectKind::Mcp => Some(MCP_CONFIG_RELPATH),
        PolicySubjectKind::Read | PolicySubjectKind::Edit => None,
    }
}

/// Why Codex cannot enforce a rule its matrix covers: an MCP rule with a
/// `*`, since `disabled_tools` and a tool's `approval_mode` take exact
/// names (`ToolFilter` in codex-rs 0.154.0 compares names as a set).
pub fn rule_gap(rule: &PolicyRule) -> Result<Option<String>> {
    Ok(match rule.subject()? {
        PolicySubject::Mcp { server, tool } if server.contains('*') || tool.contains('*') => {
            Some(
                "Codex names MCP servers and tools exactly in disabled_tools and approval_mode, so a pattern with '*' has no Codex form"
                    .to_string(),
            )
        }
        _ => None,
    })
}

/// The native rule one policy rule compiles to, or `None` for a kind of
/// rule Codex cannot express.
///
/// A command rule becomes a rules file entry: `deny` becomes
/// `decision = "forbidden"` and `ask` becomes `decision = "prompt"`, and the
/// rule's `reason`, when given, becomes the `justification` Codex shows when
/// it refuses the command. An MCP tool rule becomes `<server>:<tool>`, which
/// the policy module writes into `.codex/config.toml`.
pub fn permission_rules(rule: &PolicyRule) -> Result<Option<Vec<String>>> {
    let arguments = match rule.subject()? {
        PolicySubject::Command(arguments) => arguments,
        PolicySubject::Mcp { server, tool } => {
            if rule_gap(rule)?.is_some() {
                return Ok(None);
            }
            return Ok(Some(vec![format!("{server}:{tool}")]));
        }
        PolicySubject::Read(_) | PolicySubject::Edit(_) => return Ok(None),
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

/// Codex's hook events, from its hooks documentation. The old snake_case
/// names Tuff wrote before 0.12.0 (`pre_tool_execution`, `before_finish`,
/// `after_save`) stay as aliases, so a manifest that names one still
/// resolves.
pub const HOOK_COMPATIBILITY: CompatibilityMatrix = CompatibilityMatrix {
    spec_version: SPEC_VERSION,
    adapter: ID,
    events: &[
        CompatibilityEntry {
            event: HookEvent::SessionStart,
            native_event: Some("SessionStart"),
            aliases: &["SessionStart"],
            coverage: CoverageLevel::Full,
            scope: &["session lifecycle"],
            caveat: None,
            source: Some(CODEX_HOOKS_DOCS),
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
            source: Some(CODEX_HOOKS_DOCS),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::PreToolUse,
            native_event: Some("PreToolUse"),
            aliases: &["PreToolUse", "pre_tool_execution"],
            coverage: CoverageLevel::Full,
            scope: &["tool calls"],
            caveat: None,
            source: Some(CODEX_HOOKS_DOCS),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::PostToolUse,
            native_event: Some("PostToolUse"),
            aliases: &["PostToolUse", "post_tool_execution"],
            coverage: CoverageLevel::Full,
            scope: &["tool calls"],
            caveat: None,
            source: Some(CODEX_HOOKS_DOCS),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::BeforeFinish,
            native_event: Some("Stop"),
            aliases: &["before_finish"],
            coverage: CoverageLevel::Partial,
            scope: &["main-agent completion"],
            caveat: Some(
                "Codex Stop runs after the agent finishes responding and can request continuation; it does not represent every possible pre-finish boundary.",
            ),
            source: Some(CODEX_HOOKS_DOCS),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::AfterSave,
            native_event: None,
            aliases: &["after_save"],
            coverage: CoverageLevel::Unsupported,
            scope: &[],
            caveat: Some(
                "Codex documents no after-save event; PostToolUse on its edit tools is the closest moment.",
            ),
            source: Some(CODEX_HOOKS_DOCS),
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
            source: Some(CODEX_HOOKS_DOCS),
            since_harness_version: None,
            until_harness_version: None,
        },
    ],
};

/// The `[mcp_servers.<id>]` table Codex reads, or a refusal for a
/// declaration its config cannot carry.
///
/// Codex forwards a variable from the user's environment under its own
/// name (`env_vars`), so a declaration that renames one is refused rather
/// than written as a literal `${VAR}` Codex would not expand. A header is
/// either a bare variable (`env_http_headers`) or `Authorization: Bearer`
/// (`bearer_token_env_var`); any other format is refused.
pub fn mcp_server_entry(server: &McpServerConfig) -> Result<serde_json::Value> {
    match server.transport {
        McpTransport::Stdio => {
            let mut entry = serde_json::json!({
                "command": server.command.clone().unwrap_or_default(),
                "args": server.args,
            });
            let mut forwarded = Vec::new();
            for (name, reference) in &server.env {
                if *name != reference.from_env {
                    return Err(TuffError::unsupported(format!(
                        "Codex forwards an environment variable under its own name, so it cannot give the server '{name}' from '{}'",
                        reference.from_env
                    ))
                    .with_hint(format!(
                        "export {name} itself before starting Codex, and declare from_env = \"{name}\""
                    )));
                }
                forwarded.push(name.clone());
            }
            if !forwarded.is_empty() {
                entry["env_vars"] = serde_json::Value::from(forwarded);
            }
            Ok(entry)
        }
        McpTransport::Http => {
            let mut entry = serde_json::json!({
                "url": server.url.clone().unwrap_or_default(),
            });
            let mut plain: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
            for (name, reference) in &server.headers {
                match reference.format.as_deref() {
                    None => {
                        plain.insert(
                            name.clone(),
                            serde_json::Value::String(reference.from_env.clone()),
                        );
                    }
                    Some("Bearer {}") if name.eq_ignore_ascii_case("authorization") => {
                        entry["bearer_token_env_var"] =
                            serde_json::Value::String(reference.from_env.clone());
                    }
                    Some(format) => {
                        return Err(TuffError::unsupported(format!(
                            "Codex cannot build the header '{name}' as '{format}' from a variable; it sends a variable's value as the whole header, or a bearer token in Authorization"
                        ))
                        .with_hint("drop the format, or use Authorization with format = \"Bearer {}\""));
                    }
                }
            }
            if !plain.is_empty() {
                entry["env_http_headers"] = serde_json::Value::Object(plain);
            }
            Ok(entry)
        }
    }
}

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
        MCP_CONFIG_RELPATH
    }

    fn mcp_server_entry_checked(&self, server: &McpServerConfig) -> Result<serde_json::Value> {
        mcp_server_entry(server)
    }

    fn install_note(&self, kind: CapabilityType) -> Option<&'static str> {
        match kind {
            CapabilityType::Hook => Some(
                "Codex runs a project's hooks only in a trusted project, and only after they are approved: review them with /hooks in Codex, or start Codex with --dangerously-bypass-hook-trust in automation that vets its own hooks",
            ),
            CapabilityType::McpServer | CapabilityType::Tool => Some(
                "Codex loads a project's MCP servers from .codex/config.toml only in a trusted project",
            ),
            _ => None,
        }
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
        "SessionStart"
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

    fn permission_relpath_for(&self, subject: PolicySubjectKind) -> Option<&'static str> {
        permission_relpath(subject)
    }

    fn policy_rule_gap(&self, rule: &PolicyRule) -> Result<Option<String>> {
        rule_gap(rule)
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

        let entry = Codex.mcp_server_entry_checked(&server).unwrap();

        // Codex has no `type`; a bearer Authorization header is a token
        // variable, and any other header a variable sent whole.
        assert_eq!(
            entry,
            serde_json::json!({
                "url": "https://mcp.example.test/mcp",
                "bearer_token_env_var": "EXAMPLE_TOKEN",
            })
        );
    }

    #[test]
    fn a_stdio_server_forwards_its_variables_by_name_and_refuses_a_rename() {
        use tuff_core::manifest::EnvRef;
        let mut server = tuff_core::manifest::McpServerConfig {
            transport: tuff_core::manifest::McpTransport::Stdio,
            command: Some("npx".to_string()),
            args: vec!["-y".to_string(), "pkg".to_string()],
            url: None,
            env: [(
                "GITHUB_PERSONAL_ACCESS_TOKEN".to_string(),
                EnvRef {
                    from_env: "GITHUB_PERSONAL_ACCESS_TOKEN".to_string(),
                },
            )]
            .into_iter()
            .collect(),
            headers: Default::default(),
            metadata: None,
        };
        assert_eq!(
            Codex.mcp_server_entry_checked(&server).unwrap(),
            serde_json::json!({
                "command": "npx",
                "args": ["-y", "pkg"],
                "env_vars": ["GITHUB_PERSONAL_ACCESS_TOKEN"],
            })
        );

        server.env.insert(
            "API_KEY".to_string(),
            EnvRef {
                from_env: "MY_OTHER_KEY".to_string(),
            },
        );
        let error = Codex.mcp_server_entry_checked(&server).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("cannot give the server 'API_KEY'"),
            "{error}"
        );
    }

    #[test]
    fn a_plain_header_variable_is_sent_whole_and_a_formatted_one_is_refused() {
        use tuff_core::manifest::HeaderRef;
        let header = |name: &str, format: Option<&str>| {
            (
                name.to_string(),
                HeaderRef {
                    from_env: "KEY".to_string(),
                    format: format.map(str::to_string),
                },
            )
        };
        let mut server = tuff_core::manifest::McpServerConfig {
            transport: tuff_core::manifest::McpTransport::Http,
            command: None,
            args: Vec::new(),
            url: Some("https://mcp.example.test/mcp".to_string()),
            env: Default::default(),
            headers: [header("X-Api-Key", None)].into_iter().collect(),
            metadata: None,
        };
        assert_eq!(
            Codex.mcp_server_entry_checked(&server).unwrap(),
            serde_json::json!({
                "url": "https://mcp.example.test/mcp",
                "env_http_headers": {"X-Api-Key": "KEY"},
            })
        );
        server.headers.extend([header("X-Signed", Some("HMAC {}"))]);
        let error = Codex.mcp_server_entry_checked(&server).unwrap_err();
        assert!(
            error.to_string().contains("'X-Signed' as 'HMAC {}'"),
            "{error}"
        );
    }

    #[test]
    fn hooks_register_in_dot_codex_with_codexs_event_names() {
        assert_eq!(Codex.hook_settings_relpath(), ".codex/hooks.json");
        assert_eq!(Codex.mcp_config_relpath(), ".codex/config.toml");
        let native = |event: &str| {
            HOOK_COMPATIBILITY
                .find_event(event)
                .and_then(|entry| entry.native_event_name())
        };
        assert_eq!(native("pre_tool_use"), Some("PreToolUse"));
        assert_eq!(
            native("pre_tool_execution"),
            Some("PreToolUse"),
            "old alias"
        );
        assert_eq!(native("before_finish"), Some("Stop"));
        assert_eq!(native("session_start"), Some("SessionStart"));
        assert_eq!(native("after_save"), None);
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
            PolicyRule {
                mcp: Some("github:delete_repo".to_string()),
                ..rule("deny")
            },
            PolicyRule {
                mcp: Some("github:merge_pull_request".to_string()),
                ..rule("ask")
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
                Some(vec!["github:delete_repo".to_string()]),
                Some(vec!["github:merge_pull_request".to_string()]),
            ]
        );
        let gaps: Vec<_> = rules
            .iter()
            .map(|rule| rule_gap(rule).unwrap().is_some())
            .collect();
        assert_eq!(gaps, vec![false, false, false, true, false, false]);
        assert_eq!(
            permission_relpath(PolicySubjectKind::Mcp),
            Some(MCP_CONFIG_RELPATH)
        );
        assert_eq!(
            permission_relpath(PolicySubjectKind::Command),
            Some(RULES_RELPATH)
        );
        assert_eq!(permission_relpath(PolicySubjectKind::Read), None);
    }

    #[test]
    fn the_policy_matrix_enforces_command_and_mcp_rules() {
        let matrix = Codex.policy_compatibility();
        assert_eq!(matrix.len(), 8);
        for entry in &matrix {
            let expected = if matches!(
                entry.subject,
                PolicySubjectKind::Command | PolicySubjectKind::Mcp
            ) {
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
        assert_eq!(SUPPORTED_TYPES.len(), 5);
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
