//! The Streamable HTTP side of `tuff mcp doctor` (RFC-106 milestone 2).
//!
//! Same three steps as the stdio probe in [`crate::mcp_client`]:
//! `initialize`, `notifications/initialized`, `tools/list`. The transport
//! underneath is what differs, and it differs in three ways that each cost
//! code here:
//!
//! 1. **Two response shapes.** A server may answer a POST with a plain
//!    `application/json` body, or with `text/event-stream` carrying the
//!    same JSON-RPC message inside an SSE `data:` field. Both are legal and
//!    real servers pick either, so both are parsed.
//! 2. **A session id.** The server may return `Mcp-Session-Id` on
//!    initialize, and every later request must echo it back or be refused.
//! 3. **A negotiated protocol version.** After initialize, requests carry
//!    `MCP-Protocol-Version` naming the version the server agreed to.
//!
//! The stream is read incrementally rather than to completion, because a
//! server is only *encouraged* to close the stream once it has answered.
//! Reading to the end would turn a working server that holds the stream
//! open into a `timeout`, which is exactly the wrong answer.
//!
//! Failures are classified rather than lumped together: see [`Failure`].
//! A credential problem and an unreachable host need entirely different
//! things from the person reading the table.

use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};

use tuff_core::manifest::McpServerConfig;

/// The protocol version Tuff opens with. The server answers with the
/// version it will actually speak, which is what later requests carry.
const CLIENT_PROTOCOL_VERSION: &str = "2024-11-05";

const SESSION_HEADER: &str = "mcp-session-id";
const PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";

/// Why a probe did not reach `ok`. Each variant maps to one doctor status,
/// because the fix for each is different: a wrong token, a host that is not
/// there, and a server that answered with something that is not MCP are
/// three separate problems and folding them together would send people
/// looking in the wrong place.
pub enum Failure {
    /// 401 or 403. The token is wrong, expired, or lacks scope.
    Unauthorized(String),
    /// DNS, TLS, or connection failure — nothing answered.
    Unreachable(String),
    /// Something answered, but not with a valid MCP handshake.
    Protocol(String),
}

impl Failure {
    pub fn status(&self) -> &'static str {
        match self {
            Self::Unauthorized(_) => "unauthorized",
            Self::Unreachable(_) => "unreachable",
            Self::Protocol(_) => "protocol error",
        }
    }

    pub fn detail(self) -> String {
        match self {
            Self::Unauthorized(detail) | Self::Unreachable(detail) | Self::Protocol(detail) => {
                detail
            }
        }
    }
}

type ProbeResult<T> = std::result::Result<T, Failure>;

/// Complete the handshake against a remote server and return its tool names.
///
/// `timeout` bounds each individual request; the caller bounds the whole
/// probe as well, so a server that answers each step slowly still ends.
pub async fn handshake(server: &McpServerConfig, timeout: Duration) -> ProbeResult<Vec<String>> {
    let url = server
        .url
        .as_deref()
        .filter(|url| !url.trim().is_empty())
        .ok_or_else(|| Failure::Protocol("no url configured".to_string()))?;

    let mut session = Session::new(server, url, timeout)?;

    let initialize = session
        .request(
            1,
            "initialize",
            serde_json::json!({
                "protocolVersion": CLIENT_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "tuff-mcp-doctor",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            }),
        )
        .await?;
    session.adopt_negotiated_version(&initialize);

    // No id, so no response is expected; servers answer 202 with no body.
    session
        .notify("notifications/initialized", serde_json::json!({}))
        .await?;

    let tools = session
        .request(2, "tools/list", serde_json::json!({}))
        .await?;
    Ok(tool_names(&tools))
}

struct Session {
    client: reqwest::Client,
    url: String,
    /// The declared auth headers, resolved to their real values. Doctor
    /// makes the request the harness would make, so it needs the value, not
    /// the `${VAR}` reference an adapter writes into a config file.
    headers: HeaderMap,
    timeout: Duration,
    id: Option<HeaderValue>,
    protocol_version: Option<HeaderValue>,
}

impl Session {
    fn new(server: &McpServerConfig, url: &str, timeout: Duration) -> ProbeResult<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("tuff/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| {
                Failure::Unreachable(format!("could not build an HTTP client: {error}"))
            })?;

        let mut headers = HeaderMap::new();
        for (name, reference) in &server.headers {
            let Ok(value) = std::env::var(&reference.from_env) else {
                // Unreachable in practice: the caller reports `missing env`
                // before getting here. Skipping beats sending a header whose
                // value would be the empty string.
                continue;
            };
            let header_name = HeaderName::try_from(name.as_str())
                .map_err(|_| Failure::Protocol(format!("'{name}' is not a valid header name")))?;
            let mut header_value =
                HeaderValue::try_from(reference.render(&value)).map_err(|_| {
                    Failure::Protocol(format!(
                        "the value of {} is not valid in an HTTP header",
                        reference.from_env
                    ))
                })?;
            // Keeps the token out of any debug rendering of the map.
            header_value.set_sensitive(true);
            headers.insert(header_name, header_value);
        }

        Ok(Self {
            client,
            url: url.to_string(),
            headers,
            timeout,
            id: None,
            protocol_version: None,
        })
    }

    /// The version the server said it would speak, echoed on later
    /// requests as the spec requires.
    fn adopt_negotiated_version(&mut self, initialize_result: &serde_json::Value) {
        self.protocol_version = initialize_result
            .get("protocolVersion")
            .and_then(serde_json::Value::as_str)
            .and_then(|version| HeaderValue::try_from(version).ok());
    }

    async fn request(
        &mut self,
        id: i64,
        method: &str,
        params: serde_json::Value,
    ) -> ProbeResult<serde_json::Value> {
        let body =
            serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let response = self.post(&body).await?;
        self.remember_session(&response);
        let message = self.read_message(response, id).await?;

        if let Some(error) = message.get("error") {
            return Err(Failure::Protocol(format!(
                "{method} returned an error: {error}"
            )));
        }
        Ok(message
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

    async fn notify(&mut self, method: &str, params: serde_json::Value) -> ProbeResult<()> {
        let body = serde_json::json!({"jsonrpc": "2.0", "method": method, "params": params});
        self.post(&body).await.map(|_| ())
    }

    async fn post(&self, body: &serde_json::Value) -> ProbeResult<reqwest::Response> {
        let mut request = self
            .client
            .post(&self.url)
            .headers(self.headers.clone())
            .header(CONTENT_TYPE, "application/json")
            // Either shape is acceptable; the server chooses.
            .header(ACCEPT, "application/json, text/event-stream")
            .timeout(self.timeout)
            .json(body);
        if let Some(id) = &self.id {
            request = request.header(SESSION_HEADER, id.clone());
        }
        if let Some(version) = &self.protocol_version {
            request = request.header(PROTOCOL_VERSION_HEADER, version.clone());
        }

        let response = request.send().await.map_err(classify_transport_error)?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(Failure::Unauthorized(
                unauthorized_detail(status, response).await,
            ));
        }
        if !status.is_success() {
            return Err(Failure::Protocol(format!("the server returned {status}")));
        }
        Ok(response)
    }

    fn remember_session(&mut self, response: &reqwest::Response) {
        if let Some(id) = response.headers().get(SESSION_HEADER) {
            self.id = Some(id.clone());
        }
    }

    /// Read whichever shape the server chose until the JSON-RPC message
    /// answering `expected_id` arrives.
    async fn read_message(
        &self,
        response: reqwest::Response,
        expected_id: i64,
    ) -> ProbeResult<serde_json::Value> {
        if is_event_stream(&response) {
            return read_event_stream(response, expected_id).await;
        }
        let body = response
            .text()
            .await
            .map_err(|error| Failure::Protocol(format!("could not read the response: {error}")))?;
        let message: serde_json::Value = serde_json::from_str(&body).map_err(|error| {
            Failure::Protocol(format!("the response was not valid JSON: {error}"))
        })?;
        match_response(message, expected_id)
            .ok_or_else(|| Failure::Protocol("the response did not answer the request".to_string()))
    }
}

fn is_event_stream(response: &reqwest::Response) -> bool {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/event-stream"))
}

/// Consume SSE frames until one carries the answer, then stop — without
/// waiting for the server to close a stream it is only encouraged to close.
async fn read_event_stream(
    response: reqwest::Response,
    expected_id: i64,
) -> ProbeResult<serde_json::Value> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .map_err(|error| Failure::Protocol(format!("the event stream failed: {error}")))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(split) = next_event_boundary(&buffer) {
            let (event, rest) = buffer.split_at(split.event_end);
            let event = event.to_string();
            buffer = rest[split.separator_len..].to_string();
            if let Some(message) = event_payload(&event)
                .and_then(|payload| serde_json::from_str(&payload).ok())
                .and_then(|message| match_response(message, expected_id))
            {
                return Ok(message);
            }
        }
    }

    Err(Failure::Protocol(
        "the event stream ended before answering the request".to_string(),
    ))
}

struct EventBoundary {
    event_end: usize,
    separator_len: usize,
}

/// SSE separates events with a blank line, which is `\n\n` or `\r\n\r\n`
/// depending on who wrote the server.
fn next_event_boundary(buffer: &str) -> Option<EventBoundary> {
    let lf = buffer.find("\n\n").map(|at| EventBoundary {
        event_end: at,
        separator_len: 2,
    });
    let crlf = buffer.find("\r\n\r\n").map(|at| EventBoundary {
        event_end: at,
        separator_len: 4,
    });
    match (lf, crlf) {
        (Some(lf), Some(crlf)) if crlf.event_end <= lf.event_end => Some(crlf),
        (Some(lf), _) => Some(lf),
        (None, other) => other,
    }
}

/// The `data:` lines of one SSE event, joined as the spec requires. Every
/// other field (`event:`, `id:`, `retry:`, comments) is not the payload.
fn event_payload(event: &str) -> Option<String> {
    let mut data: Vec<&str> = Vec::new();
    for line in event.lines() {
        if let Some(value) = line.strip_prefix("data:") {
            data.push(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    (!data.is_empty()).then(|| data.join("\n"))
}

/// `Some` when this message is the response to `expected_id`. Anything else
/// — a notification, a request the server made of us, another id — is not
/// an error, it is simply not the message being waited on.
fn match_response(message: serde_json::Value, expected_id: i64) -> Option<serde_json::Value> {
    (message.get("id").and_then(serde_json::Value::as_i64) == Some(expected_id)).then_some(message)
}

fn classify_transport_error(error: reqwest::Error) -> Failure {
    if error.is_timeout() {
        return Failure::Unreachable(format!("the server did not respond in time: {error}"));
    }
    if error.is_connect() || error.is_request() {
        return Failure::Unreachable(error.to_string());
    }
    Failure::Protocol(error.to_string())
}

/// A 401 usually carries a `WWW-Authenticate` header naming the scheme or
/// the realm, which is the most useful thing to put in front of someone
/// whose token was refused.
async fn unauthorized_detail(status: reqwest::StatusCode, response: reqwest::Response) -> String {
    let challenge = response
        .headers()
        .get(reqwest::header::WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    match challenge {
        Some(challenge) => format!("the server returned {status} ({challenge})"),
        None => format!("the server returned {status}"),
    }
}

fn tool_names(result: &serde_json::Value) -> Vec<String> {
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

    #[test]
    fn an_sse_event_payload_joins_every_data_line_and_ignores_the_rest() {
        let event = "event: message\nid: 7\ndata: {\"jsonrpc\":\ndata: continued}\n: a comment";
        assert_eq!(
            event_payload(event),
            Some("{\"jsonrpc\":\ncontinued}".to_string())
        );
        assert_eq!(event_payload("event: ping\nid: 1"), None);
        // A line that does not start with the field name is not data, even
        // when it looks like one; SSE does not allow leading whitespace.
        assert_eq!(
            event_payload("data: kept\n data: dropped"),
            Some("kept".to_string())
        );
    }

    #[test]
    fn event_boundaries_are_found_in_both_line_ending_styles() {
        let lf = next_event_boundary("data: {}\n\ndata: {}").expect("lf boundary");
        assert_eq!((lf.event_end, lf.separator_len), (8, 2));

        let crlf = next_event_boundary("data: {}\r\n\r\ndata: {}").expect("crlf boundary");
        assert_eq!((crlf.event_end, crlf.separator_len), (8, 4));

        assert!(next_event_boundary("data: {}\n").is_none());
    }

    /// A stream that carries an unrelated notification before the answer
    /// must skip it rather than treat it as a malformed response.
    #[test]
    fn only_the_message_answering_the_request_matches() {
        let notification = serde_json::json!({"jsonrpc": "2.0", "method": "notifications/message"});
        assert!(match_response(notification, 2).is_none());

        let other_id = serde_json::json!({"jsonrpc": "2.0", "id": 1, "result": {}});
        assert!(match_response(other_id, 2).is_none());

        let answer = serde_json::json!({"jsonrpc": "2.0", "id": 2, "result": {"tools": []}});
        assert!(match_response(answer, 2).is_some());
    }

    #[test]
    fn tool_names_reads_the_list_and_tolerates_anything_else() {
        let result = serde_json::json!({"tools": [{"name": "search"}, {"description": "no name"}]});
        assert_eq!(tool_names(&result), vec!["search".to_string()]);
        assert!(tool_names(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn each_failure_maps_to_its_own_doctor_status() {
        assert_eq!(
            Failure::Unauthorized(String::new()).status(),
            "unauthorized"
        );
        assert_eq!(Failure::Unreachable(String::new()).status(), "unreachable");
        assert_eq!(Failure::Protocol(String::new()).status(), "protocol error");
    }
}
