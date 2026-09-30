use std::path::Path;

use tuff_hooks_spec::{
    CompatibilityEntry, CompatibilityMatrix, CoverageLevel, HookEvent, SPEC_VERSION,
};

use tuff_core::adapter::{AgentAdapter, HookSettingsShape};
use tuff_core::error::{Result, TuffError};
use tuff_core::manifest::CapabilityType;
use tuff_core::policy::{PolicyCoverageEntry, PolicyEffect, PolicyRule, PolicySubjectKind};
use tuff_core::policy_eval::{
    PolicyAction, PolicyHookAnswer, PolicyHookRequest, PolicyHookUse, PolicyVerdict,
};

pub const ID: &str = "cursor";
pub const DISPLAY_NAME: &str = "Cursor";
pub const SUPPORTED_TYPES: &[CapabilityType] = &[
    CapabilityType::Skill,
    CapabilityType::Tool,
    CapabilityType::Hook,
    CapabilityType::McpServer,
    CapabilityType::Policy,
];

pub const SUPPORTED_AGENTS: &[&str] = &["Cursor"];

pub const HOOK_SETTINGS_RELPATH: &str = ".cursor/hooks.json";

pub struct Cursor;

const CURSOR_HOOKS_DOCS: &str = "https://cursor.com/docs/agent/hooks";

/// How Cursor enforces each kind of policy rule. Cursor has no project
/// permission rules Tuff can compile to, so every enforced rule runs through
/// `tuff policy evaluate`, registered with `failClosed` so a missing or
/// failing `tuff` blocks the call. Taken from Cursor's hooks documentation
/// on 2026-09-30; not yet observed in a running Cursor.
pub fn policy_matrix() -> Vec<PolicyCoverageEntry> {
    const HOOK: &str = "tuff policy evaluate runs as a Cursor hook, so tuff must be on the PATH of every machine where the agent runs; the hook is registered with failClosed, so a missing or failing tuff blocks the call; checked against Cursor's hooks documentation, not yet in a running Cursor";
    let command = format!(
        "{HOOK}; the command is matched after unwrapping sh -c, env, sudo, and absolute paths, and a script or program that runs the command itself is not seen"
    );
    let read = format!(
        "{HOOK}; covers the agent's file reads and the files a shell command names on its command line, and a script or program that opens a file itself is not seen"
    );
    let row =
        |effect, subject, coverage, mechanism: Option<&str>, caveat: String| PolicyCoverageEntry {
            effect,
            subject,
            coverage,
            mechanism: mechanism.map(str::to_string),
            caveat: Some(caveat),
            source: Some(CURSOR_HOOKS_DOCS.to_string()),
        };
    use CoverageLevel::{Full, Partial, Unsupported};
    use PolicyEffect::{Ask, Deny};
    use PolicySubjectKind::{Command, Edit, Mcp, Read};
    const SHELL: &str = ".cursor/hooks.json beforeShellExecution: tuff policy evaluate";
    const MCP: &str = ".cursor/hooks.json beforeMCPExecution: tuff policy evaluate";
    const NO_EDIT: &str = "Cursor's hook for file writes, preToolUse, does not document the path field of its Write tool, so Tuff does not compile edit rules for Cursor";
    vec![
        row(Deny, Command, Partial, Some(SHELL), command.clone()),
        row(
            Deny,
            Read,
            Partial,
            Some(".cursor/hooks.json beforeReadFile, beforeShellExecution: tuff policy evaluate"),
            read,
        ),
        row(Deny, Edit, Unsupported, None, NO_EDIT.to_string()),
        row(Deny, Mcp, Full, Some(MCP), HOOK.to_string()),
        row(Ask, Command, Partial, Some(SHELL), command),
        row(
            Ask,
            Read,
            Unsupported,
            None,
            "Cursor's beforeReadFile hook can allow or deny a read but cannot ask".to_string(),
        ),
        row(Ask, Edit, Unsupported, None, NO_EDIT.to_string()),
        row(Ask, Mcp, Full, Some(MCP), HOOK.to_string()),
    ]
}

pub const HOOK_COMPATIBILITY: CompatibilityMatrix = CompatibilityMatrix {
    spec_version: SPEC_VERSION,
    adapter: ID,
    events: &[
        CompatibilityEntry {
            event: HookEvent::SessionStart,
            native_event: Some("sessionStart"),
            aliases: &[],
            coverage: CoverageLevel::Full,
            scope: &[],
            caveat: None,
            source: Some("https://cursor.com/blog/agent-best-practices"),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::SessionEnd,
            native_event: Some("sessionEnd"),
            aliases: &[],
            coverage: CoverageLevel::Full,
            scope: &["session lifecycle"],
            caveat: None,
            source: Some("https://cursor.com/blog/agent-best-practices"),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::PreToolUse,
            native_event: Some("preToolUse"),
            aliases: &[],
            coverage: CoverageLevel::Full,
            scope: &["agent tool calls"],
            caveat: Some("A native matcher can narrow the tools that receive the hook."),
            source: Some("https://cursor.com/blog/agent-best-practices"),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::PostToolUse,
            native_event: Some("postToolUse"),
            aliases: &[],
            coverage: CoverageLevel::Full,
            scope: &["agent tool calls"],
            caveat: Some("A native matcher can narrow the tools that receive the hook."),
            source: Some("https://cursor.com/blog/agent-best-practices"),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::AfterSave,
            native_event: None,
            aliases: &[],
            coverage: CoverageLevel::Unsupported,
            scope: &[],
            caveat: Some("Cursor does not expose a direct after-save hook in this adapter."),
            source: None,
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::BeforeFinish,
            native_event: Some("stop"),
            aliases: &[],
            coverage: CoverageLevel::Partial,
            scope: &["agent completion"],
            caveat: Some("Cursor stop can request continuation rather than block a prior action."),
            source: Some("https://cursor.com/blog/agent-best-practices"),
            since_harness_version: None,
            until_harness_version: None,
        },
        CompatibilityEntry {
            event: HookEvent::Stop,
            native_event: Some("stop"),
            aliases: &[],
            coverage: CoverageLevel::Full,
            scope: &["agent completion"],
            caveat: None,
            source: Some("https://cursor.com/blog/agent-best-practices"),
            since_harness_version: None,
            until_harness_version: None,
        },
    ],
};

impl AgentAdapter for Cursor {
    fn id(&self) -> &'static str {
        ID
    }

    fn display_name(&self) -> &'static str {
        DISPLAY_NAME
    }

    fn dir_prefix(&self) -> &'static str {
        ".cursor"
    }

    fn mcp_config_relpath(&self) -> &'static str {
        ".cursor/mcp.json"
    }

    /// Cursor interpolates `${env:VAR}` in `mcp.json`, not the bare `${VAR}`
    /// form Claude Code and most stdio clients use.
    fn mcp_env_reference(&self, var: &str) -> String {
        format!("${{env:{var}}}")
    }

    /// Cursor's remote-server entry is `{ "url": …, "headers": … }` with no
    /// `type` key; it distinguishes stdio from remote by the presence of
    /// `command` versus `url`.
    fn mcp_http_declares_type(&self) -> bool {
        false
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
        "sessionStart"
    }

    fn hook_settings_shape(&self) -> HookSettingsShape {
        HookSettingsShape::Flat
    }

    fn detect(&self, repo_root: &Path) -> bool {
        repo_root.join(".cursor").exists()
    }

    fn policy_compatibility(&self) -> Vec<PolicyCoverageEntry> {
        policy_matrix()
    }

    fn policy_hook_use(&self, rule: &PolicyRule) -> Result<PolicyHookUse> {
        Ok(match rule.subject()?.kind() {
            PolicySubjectKind::Edit => PolicyHookUse::Never,
            _ => PolicyHookUse::Required,
        })
    }

    fn policy_hook_fragment(
        &self,
        command: &str,
        subjects: &[PolicySubjectKind],
    ) -> Option<serde_json::Value> {
        let entry = serde_json::json!([{"command": command, "failClosed": true}]);
        let mut hooks = serde_json::Map::new();
        if subjects.contains(&PolicySubjectKind::Command)
            || subjects.contains(&PolicySubjectKind::Read)
        {
            hooks.insert("beforeShellExecution".to_string(), entry.clone());
        }
        if subjects.contains(&PolicySubjectKind::Read) {
            hooks.insert("beforeReadFile".to_string(), entry.clone());
        }
        if subjects.contains(&PolicySubjectKind::Mcp) {
            hooks.insert("beforeMCPExecution".to_string(), entry);
        }
        (!hooks.is_empty()).then(|| serde_json::json!({"version": 1, "hooks": hooks}))
    }

    fn policy_hook_request(&self, input: &serde_json::Value) -> Result<PolicyHookRequest> {
        hook_request(input)
    }

    fn policy_hook_answer(&self, event: &str, verdict: PolicyVerdict<'_>) -> PolicyHookAnswer {
        hook_answer(event, verdict)
    }
}

/// Read a Cursor hook input for `beforeShellExecution`, `beforeReadFile`,
/// or `beforeMCPExecution`.
pub fn hook_request(input: &serde_json::Value) -> Result<PolicyHookRequest> {
    let text = |key: &str| {
        input
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    let event = text("hook_event_name")
        .ok_or_else(|| TuffError::usage("hook input has no hook_event_name"))?;
    let missing = |key: &str| TuffError::usage(format!("{event} hook input has no {key}"));
    let action = match event.as_str() {
        "beforeShellExecution" => {
            PolicyAction::Shell(text("command").ok_or_else(|| missing("command"))?)
        }
        "beforeReadFile" => {
            PolicyAction::Read(text("file_path").ok_or_else(|| missing("file_path"))?)
        }
        "beforeMCPExecution" => PolicyAction::Mcp {
            server: text("mcp_server_name").ok_or_else(|| missing("mcp_server_name"))?,
            tool: text("tool_name").ok_or_else(|| missing("tool_name"))?,
        },
        other => {
            return Err(TuffError::usage(format!(
                "tuff policy evaluate does not handle Cursor's {other} hook"
            )));
        }
    };
    let roots = input
        .get("workspace_roots")
        .and_then(serde_json::Value::as_array)
        .map(|roots| {
            roots
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(Into::into)
                .collect()
        })
        .unwrap_or_default();
    Ok(PolicyHookRequest {
        event,
        cwd: text("cwd").map(Into::into),
        roots,
        actions: vec![action],
    })
}

/// A Cursor hook answer, which is always a JSON `permission`. No match is
/// `allow`; `beforeReadFile` cannot ask, so an ask rule there allows the
/// read. Input that cannot be read is denied.
pub fn hook_answer(event: &str, verdict: PolicyVerdict<'_>) -> PolicyHookAnswer {
    let (permission, message) = match verdict {
        PolicyVerdict::NoMatch => ("allow", None),
        PolicyVerdict::Matched(decision) => {
            let permission = match decision.effect {
                PolicyEffect::Ask if event == "beforeReadFile" => "allow",
                effect => effect.as_str(),
            };
            (permission, Some(decision.message()))
        }
        PolicyVerdict::Failed(message) => ("deny", Some(message.to_string())),
    };
    let mut answer = serde_json::json!({"permission": permission});
    if let Some(message) = message
        && permission != "allow"
    {
        answer["user_message"] = serde_json::json!(message);
        answer["agent_message"] = serde_json::json!(message);
    }
    PolicyHookAnswer {
        stdout: answer.to_string(),
        exit_code: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC-106 D2: Cursor's remote entry has no `type` key and spells a
    /// variable `${env:VAR}`, unlike every other harness Tuff targets.
    #[test]
    fn a_remote_server_entry_omits_type_and_uses_cursor_variable_syntax() {
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

        let entry = Cursor.mcp_server_entry(&server);

        assert_eq!(
            entry,
            serde_json::json!({
                "url": "https://mcp.example.test/mcp",
                "headers": {"Authorization": "Bearer ${env:EXAMPLE_TOKEN}"},
            })
        );
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
                "beforeSubmitPrompt": [{"hooks": [{"type": "command", "command": "sh .cursor/hooks/demo/run.sh"}]}]
            }
        });

        let once = Cursor
            .merge_hook_fragment(None, &fragment)
            .expect("first merge");
        let twice = Cursor
            .merge_hook_fragment(Some(&once), &fragment)
            .expect("second merge");

        let settings: serde_json::Value = serde_json::from_slice(&twice).expect("valid json");
        let groups = settings["hooks"]["beforeSubmitPrompt"]
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
