//! A minimal MCP client for `tuff mcp doctor` (RFC-102 stage d).
//!
//! Spawns the real server, does the `initialize` handshake, and calls
//! `tools/list` — the difference between "wrote the config file" and "it
//! works." Framing is plain newline-delimited JSON-RPC over stdin/stdout,
//! per the real MCP stdio transport spec — not the `Content-Length`
//! framing `examples/tools/mcp-server-tool/server.js` used before this
//! change, which would have made doctor pass against the repo's own
//! example while failing against every real server.
//!
//! HTTP servers are probed too, by [`crate::mcp_http`], which speaks the
//! Streamable HTTP transport. This module owns the stdio half and the
//! dispatch between them.

use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{ChildStdin, ChildStdout, Command};

use tuff_core::error::{Result, TuffError};
use tuff_core::manifest::{McpServerConfig, McpTransport};

pub struct ProbeReport {
    pub status: &'static str,
    pub detail: String,
    pub tools: Vec<String>,
}

/// Environment variables the server declares that aren't set in this
/// process's environment, in a stable order. Covers `[server.headers]` as
/// well as `[server.env]`, so a remote server whose token is not exported
/// is reported before a request leaves the machine rather than after the
/// server refuses it.
pub fn unset_env_vars(server: &McpServerConfig) -> Vec<String> {
    let mut missing: Vec<String> = server
        .env
        .values()
        .map(|reference| reference.from_env.clone())
        .chain(
            server
                .headers
                .values()
                .map(|reference| reference.from_env.clone()),
        )
        .filter(|name| std::env::var(name).is_err())
        .collect();
    missing.sort();
    missing.dedup();
    missing
}

pub async fn probe(server: &McpServerConfig, timeout: Duration) -> ProbeReport {
    // Checked first, and for both transports, so a server Tuff cannot
    // authenticate is reported without anything leaving the machine.
    let missing = unset_env_vars(server);
    if !missing.is_empty() {
        return ProbeReport {
            status: "missing env",
            detail: format!("export {}", missing.join(", ")),
            tools: Vec::new(),
        };
    }

    if server.transport == McpTransport::Http {
        return probe_http(server, timeout).await;
    }

    let Some(command) = server.command.as_deref().filter(|c| !c.trim().is_empty()) else {
        return ProbeReport {
            status: "spawn failed",
            detail: "no command configured".to_string(),
            tools: Vec::new(),
        };
    };

    let mut cmd = Command::new(command);
    cmd.args(&server.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for reference in server.env.values() {
        if let Ok(value) = std::env::var(&reference.from_env) {
            cmd.env(&reference.from_env, value);
        }
    }

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(error) => {
            return ProbeReport {
                status: "spawn failed",
                detail: error.to_string(),
                tools: Vec::new(),
            };
        }
    };
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let mut lines = BufReader::new(child.stdout.take().expect("stdout is piped")).lines();

    let outcome = tokio::time::timeout(timeout, handshake(&mut stdin, &mut lines)).await;
    let _ = child.kill().await;

    match outcome {
        Ok(Ok(tools)) => ProbeReport {
            status: "ok",
            detail: format!("{} tool(s)", tools.len()),
            tools,
        },
        Ok(Err(error)) => ProbeReport {
            status: "protocol error",
            detail: error.to_string(),
            tools: Vec::new(),
        },
        Err(_) => ProbeReport {
            status: "timeout",
            detail: format!("no response within {timeout:?}"),
            tools: Vec::new(),
        },
    }
}

/// The whole HTTP probe under one deadline, so a server that answers each
/// step slowly still ends rather than multiplying the timeout by the number
/// of steps.
async fn probe_http(server: &McpServerConfig, timeout: Duration) -> ProbeReport {
    match tokio::time::timeout(timeout, crate::mcp_http::handshake(server, timeout)).await {
        Ok(Ok(tools)) => ProbeReport {
            status: "ok",
            detail: format!("{} tool(s)", tools.len()),
            tools,
        },
        Ok(Err(failure)) => ProbeReport {
            status: failure.status(),
            detail: failure.detail(),
            tools: Vec::new(),
        },
        Err(_) => ProbeReport {
            status: "timeout",
            detail: format!("no response within {timeout:?}"),
            tools: Vec::new(),
        },
    }
}

async fn handshake(
    stdin: &mut ChildStdin,
    lines: &mut Lines<BufReader<ChildStdout>>,
) -> Result<Vec<String>> {
    send_request(
        stdin,
        1,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "tuff-mcp-doctor", "version": env!("CARGO_PKG_VERSION")},
        }),
    )
    .await?;
    read_response(lines, 1).await?;

    // Real servers may refuse further calls until this notification lands —
    // no id, no response expected.
    send_notification(stdin, "notifications/initialized", serde_json::json!({})).await?;

    send_request(stdin, 2, "tools/list", serde_json::json!({})).await?;
    let result = read_response(lines, 2).await?;
    Ok(parse_tool_names(&result))
}

async fn send_request(
    stdin: &mut ChildStdin,
    id: i64,
    method: &str,
    params: serde_json::Value,
) -> Result<()> {
    write_line(
        stdin,
        &serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
    )
    .await
}

async fn send_notification(
    stdin: &mut ChildStdin,
    method: &str,
    params: serde_json::Value,
) -> Result<()> {
    write_line(
        stdin,
        &serde_json::json!({"jsonrpc": "2.0", "method": method, "params": params}),
    )
    .await
}

async fn write_line(stdin: &mut ChildStdin, message: &serde_json::Value) -> Result<()> {
    let mut line = serde_json::to_vec(message)?;
    line.push(b'\n');
    stdin.write_all(&line).await?;
    stdin.flush().await?;
    Ok(())
}

/// Read lines until one is a JSON-RPC response matching `expected_id`,
/// skipping anything else — blank lines, notifications, or (real servers
/// occasionally misbehave) incidental non-protocol stdout noise.
async fn read_response(
    lines: &mut Lines<BufReader<ChildStdout>>,
    expected_id: i64,
) -> Result<serde_json::Value> {
    loop {
        let Some(line) = lines.next_line().await? else {
            return Err(TuffError::source_failed(
                "server closed stdout before responding",
            ));
        };
        let Some(message) = parse_response_line(&line, expected_id) else {
            continue;
        };
        return message;
    }
}

/// Parse one line as a JSON-RPC message; `None` means "not a response to
/// `expected_id`, keep reading" — not an error, since it might be a
/// notification, a mismatched id, or non-protocol noise.
fn parse_response_line(line: &str, expected_id: i64) -> Option<Result<serde_json::Value>> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let message: serde_json::Value = serde_json::from_str(line).ok()?;
    if message.get("id").and_then(serde_json::Value::as_i64) != Some(expected_id) {
        return None;
    }
    if let Some(error) = message.get("error") {
        return Some(Err(TuffError::source_failed(format!(
            "server returned an error: {error}"
        ))));
    }
    Some(Ok(message
        .get("result")
        .cloned()
        .unwrap_or(serde_json::Value::Null)))
}

fn parse_tool_names(result: &serde_json::Value) -> Vec<String> {
    result
        .get("tools")
        .and_then(serde_json::Value::as_array)
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool.get("name")?.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tuff_core::manifest::EnvRef;

    fn server_with_env(names: &[&str]) -> McpServerConfig {
        McpServerConfig {
            transport: McpTransport::Stdio,
            command: Some("npx".to_string()),
            args: Vec::new(),
            url: None,
            env: names
                .iter()
                .map(|name| {
                    (
                        name.to_string(),
                        EnvRef {
                            from_env: name.to_string(),
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>(),
            headers: BTreeMap::new(),
            metadata: None,
        }
    }

    #[test]
    fn unset_env_vars_reports_only_missing_names() {
        // SAFETY: single-threaded test, no other test reads this exact name.
        unsafe {
            std::env::set_var("TUFF_DOCTOR_TEST_PRESENT", "1");
            std::env::remove_var("TUFF_DOCTOR_TEST_ABSENT");
        }
        let server = server_with_env(&["TUFF_DOCTOR_TEST_PRESENT", "TUFF_DOCTOR_TEST_ABSENT"]);
        assert_eq!(
            unset_env_vars(&server),
            vec!["TUFF_DOCTOR_TEST_ABSENT".to_string()]
        );
    }

    #[test]
    fn unset_env_vars_empty_when_nothing_declared() {
        let server = server_with_env(&[]);
        assert!(unset_env_vars(&server).is_empty());
    }

    #[test]
    fn parse_response_line_matches_id_and_extracts_result() {
        let parsed = parse_response_line(r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[]}}"#, 2);
        assert_eq!(parsed.unwrap().unwrap(), serde_json::json!({"tools": []}));
    }

    #[test]
    fn parse_response_line_skips_mismatched_id_and_notifications() {
        assert!(parse_response_line(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#, 2).is_none());
        assert!(
            parse_response_line(r#"{"jsonrpc":"2.0","method":"log","params":{}}"#, 2).is_none()
        );
        assert!(parse_response_line("", 2).is_none());
        assert!(parse_response_line("not json at all", 2).is_none());
    }

    #[test]
    fn parse_response_line_turns_an_error_object_into_err() {
        let parsed = parse_response_line(
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"nope"}}"#,
            1,
        );
        assert!(parsed.unwrap().is_err());
    }

    #[test]
    fn parse_tool_names_extracts_names_and_ignores_missing_field() {
        let result =
            serde_json::json!({"tools": [{"name": "a"}, {"name": "b"}, {"no_name": true}]});
        assert_eq!(
            parse_tool_names(&result),
            vec!["a".to_string(), "b".to_string()]
        );
        assert!(parse_tool_names(&serde_json::json!({})).is_empty());
    }
}
