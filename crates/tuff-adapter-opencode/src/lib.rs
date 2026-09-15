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

pub const ID: &str = "opencode";
pub const DISPLAY_NAME: &str = "OpenCode";

/// Policies only. Skills reach OpenCode through the `open-agents` layout;
/// OpenCode MCP servers and hooks are not managed by this adapter yet.
pub const SUPPORTED_TYPES: &[CapabilityType] = &[CapabilityType::Policy];

pub const SUPPORTED_AGENTS: &[&str] = &["OpenCode"];

/// The OpenCode config file policy rules are compiled into. OpenCode loads
/// `.opencode/opencode.json` after the project's `opencode.json` and applies
/// the last permission rule that matches, so rules here take precedence over
/// the project's own.
pub const CONFIG_RELPATH: &str = ".opencode/opencode.json";
const OPENCODE_PERMISSIONS_DOCS: &str = "https://opencode.ai/docs/permissions/";

pub struct OpenCode;

const fn unsupported_hook(event: HookEvent) -> CompatibilityEntry {
    CompatibilityEntry {
        event,
        native_event: None,
        aliases: &[],
        coverage: CoverageLevel::Unsupported,
        scope: &[],
        caveat: Some("Tuff does not manage OpenCode hooks, which OpenCode loads as plugins."),
        source: None,
        since_harness_version: None,
        until_harness_version: None,
    }
}

/// Every event is unsupported: this adapter takes no hooks, and
/// `tuff hooks spec` lists only adapters that do.
pub const HOOK_COMPATIBILITY: CompatibilityMatrix = CompatibilityMatrix {
    spec_version: SPEC_VERSION,
    adapter: ID,
    events: &[
        unsupported_hook(HookEvent::SessionStart),
        unsupported_hook(HookEvent::SessionEnd),
        unsupported_hook(HookEvent::PreToolUse),
        unsupported_hook(HookEvent::PostToolUse),
        unsupported_hook(HookEvent::BeforeFinish),
        unsupported_hook(HookEvent::AfterSave),
        unsupported_hook(HookEvent::Stop),
    ],
};

/// How OpenCode enforces each kind of policy rule, from its permissions
/// documentation and source, checked against OpenCode 1.18.15 on 2026-09-15.
pub fn policy_matrix() -> Vec<PolicyCoverageEntry> {
    const PRECEDENCE: &str = "OpenCode loads .opencode/opencode.json after the project's opencode.json, but inline OPENCODE_CONFIG_CONTENT, managed config, and an agent's own permission settings are applied after it";
    const COMMAND: &str = "matches each command OpenCode parses from the shell input, including one with a redirection; the same program run through sh -c, by absolute path, or with options before the subcommand is not matched";
    const READ: &str = "covers OpenCode's read tool; grep, glob, list, and shell commands are separate permissions and are not covered";
    const EDIT: &str = "covers OpenCode's edit, write, and patch tools; shell commands that write files are not covered";
    const MCP_DENY: &str = "a denied tool is hidden from the agent; OpenCode names a tool <server>_<tool>, with characters other than letters, digits, _ and - replaced by _";
    const MCP_ASK: &str = "OpenCode names a tool <server>_<tool>, with characters other than letters, digits, _ and - replaced by _";
    const ASK: &str = "opencode run rejects the request, and opencode --auto approves it";
    let row = |effect, subject, coverage, mechanism: &str, caveat: String| PolicyCoverageEntry {
        effect,
        subject,
        coverage,
        mechanism: Some(mechanism.to_string()),
        caveat: Some(caveat),
        source: Some(OPENCODE_PERMISSIONS_DOCS.to_string()),
    };
    use CoverageLevel::{Full, Partial};
    use PolicyEffect::{Ask, Deny};
    use PolicySubjectKind::{Command, Edit, Mcp, Read};
    vec![
        row(
            Deny,
            Command,
            Partial,
            ".opencode/opencode.json permission.bash \"<command> *\": \"deny\"",
            format!("{COMMAND}; {PRECEDENCE}"),
        ),
        row(
            Deny,
            Read,
            Partial,
            ".opencode/opencode.json permission.read \"<path>\": \"deny\"",
            format!("{READ}; {PRECEDENCE}"),
        ),
        row(
            Deny,
            Edit,
            Partial,
            ".opencode/opencode.json permission.edit \"<path>\": \"deny\"",
            format!("{EDIT}; {PRECEDENCE}"),
        ),
        row(
            Deny,
            Mcp,
            Full,
            ".opencode/opencode.json permission \"<server>_<tool>\": \"deny\"",
            format!("{MCP_DENY}; {PRECEDENCE}"),
        ),
        row(
            Ask,
            Command,
            Partial,
            ".opencode/opencode.json permission.bash \"<command> *\": \"ask\"",
            format!("{COMMAND}; {ASK}; {PRECEDENCE}"),
        ),
        row(
            Ask,
            Read,
            Partial,
            ".opencode/opencode.json permission.read \"<path>\": \"ask\"",
            format!("{READ}; {ASK}; {PRECEDENCE}"),
        ),
        row(
            Ask,
            Edit,
            Partial,
            ".opencode/opencode.json permission.edit \"<path>\": \"ask\"",
            format!("{EDIT}; {ASK}; {PRECEDENCE}"),
        ),
        row(
            Ask,
            Mcp,
            Full,
            ".opencode/opencode.json permission \"<server>_<tool>\": \"ask\"",
            format!("{MCP_ASK}; {ASK}; {PRECEDENCE}"),
        ),
    ]
}

/// A policy path pattern, read as `.gitignore` reads a pattern at the
/// project root, as OpenCode path patterns.
///
/// OpenCode matches a read or edit against the path relative to the
/// project, and its `*` also matches `/`. A pattern with a `/` at its start
/// or middle stays anchored, so `secrets/**` is kept. One without matches at
/// any depth, which takes two patterns: `.env` and `*/.env`. A trailing `/`
/// names a directory's contents.
pub fn permission_paths(pattern: &str) -> Vec<String> {
    let pattern = pattern.trim_start_matches("./");
    let anchored = pattern.trim_end_matches('/').contains('/');
    let normalized = if pattern.ends_with('/') {
        format!("{pattern}*")
    } else {
        pattern.to_string()
    };
    if anchored {
        vec![normalized]
    } else {
        vec![normalized.clone(), format!("*/{normalized}")]
    }
}

/// A server or tool name as OpenCode writes it into a tool's permission
/// name: characters other than letters, digits, `_`, and `-` become `_`. A
/// policy's `*` is kept, since OpenCode matches permission names as
/// patterns.
fn tool_name_part(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '*') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

/// The OpenCode rules one policy rule compiles to, as Tuff records them: a
/// permission name, then a space and a pattern when the rule sits in that
/// permission's object. The effect is the action the rule is written with.
pub fn permission_rules(rule: &PolicyRule) -> Result<Vec<String>> {
    Ok(match rule.subject()? {
        PolicySubject::Command(arguments) => vec![format!("bash {} *", arguments.join(" "))],
        PolicySubject::Read(patterns) => patterns
            .iter()
            .flat_map(|pattern| permission_paths(pattern))
            .map(|pattern| format!("read {pattern}"))
            .collect(),
        PolicySubject::Edit(patterns) => patterns
            .iter()
            .flat_map(|pattern| permission_paths(pattern))
            .map(|pattern| format!("edit {pattern}"))
            .collect(),
        PolicySubject::Mcp { server, tool } => {
            vec![format!(
                "{}_{}",
                tool_name_part(server),
                tool_name_part(tool)
            )]
        }
    })
}

impl AgentAdapter for OpenCode {
    fn id(&self) -> &'static str {
        ID
    }

    fn display_name(&self) -> &'static str {
        DISPLAY_NAME
    }

    fn dir_prefix(&self) -> &'static str {
        ".opencode"
    }

    fn mcp_config_relpath(&self) -> &'static str {
        "opencode.json"
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
        CONFIG_RELPATH
    }

    fn scaffold_hook_event(&self) -> &'static str {
        ""
    }

    fn hook_settings_shape(&self) -> HookSettingsShape {
        HookSettingsShape::Flat
    }

    fn policy_compatibility(&self) -> Vec<PolicyCoverageEntry> {
        policy_matrix()
    }

    fn permissions_settings_relpath(&self) -> Option<&'static str> {
        Some(CONFIG_RELPATH)
    }

    fn native_permission_rules(&self, rule: &PolicyRule) -> Result<Option<Vec<String>>> {
        permission_rules(rule).map(Some)
    }

    fn detect(&self, repo_root: &Path) -> bool {
        repo_root.join("opencode.json").exists()
            || repo_root.join("opencode.jsonc").exists()
            || repo_root.join(".opencode").exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn words(words: &[&str]) -> Option<Vec<String>> {
        Some(words.iter().map(|word| word.to_string()).collect())
    }

    #[test]
    fn each_kind_of_rule_compiles_to_an_opencode_permission() {
        let compiled = |rule: PolicyRule| permission_rules(&rule).unwrap();
        assert_eq!(
            compiled(PolicyRule {
                command: words(&["git", "push", "--force"]),
                ..rule("deny")
            }),
            ["bash git push --force *"]
        );
        assert_eq!(
            compiled(PolicyRule {
                read: words(&[".env", "secrets/**"]),
                ..rule("deny")
            }),
            ["read .env", "read */.env", "read secrets/**"]
        );
        assert_eq!(
            compiled(PolicyRule {
                edit: words(&["infra/prod/", "certs/"]),
                ..rule("ask")
            }),
            ["edit infra/prod/*", "edit certs/*", "edit */certs/*"]
        );
        assert_eq!(
            compiled(PolicyRule {
                mcp: Some("my.server:delete_*".to_string()),
                ..rule("deny")
            }),
            ["my_server_delete_*"]
        );
    }

    #[test]
    fn the_policy_matrix_enforces_every_kind_of_rule() {
        let matrix = OpenCode.policy_compatibility();
        assert_eq!(matrix.len(), 8);
        for entry in &matrix {
            let expected = if entry.subject == PolicySubjectKind::Mcp {
                CoverageLevel::Full
            } else {
                CoverageLevel::Partial
            };
            assert_eq!(entry.coverage, expected, "{entry:?}");
            assert!(entry.caveat.is_some(), "{entry:?}");
        }
    }

    #[test]
    fn opencode_is_a_policy_only_target() {
        assert_eq!(SUPPORTED_TYPES, [CapabilityType::Policy]);
        assert!(
            HOOK_COMPATIBILITY
                .events
                .iter()
                .all(|entry| entry.coverage == CoverageLevel::Unsupported)
        );
    }
}
