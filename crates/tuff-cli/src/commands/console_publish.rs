//! `tuff console publish` (RFC-108 D10): build a report for each project
//! and send it to a console server, with an API key or, inside GitHub
//! Actions, with the job's OIDC token.

use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use tuff_core::report::Report;

use crate::error::{Result, TuffError};

use super::block_on_oci;

/// Environment variables the GitHub Actions runner sets for jobs that have
/// `permissions: id-token: write`.
const OIDC_URL_VAR: &str = "ACTIONS_ID_TOKEN_REQUEST_URL";
const OIDC_TOKEN_VAR: &str = "ACTIONS_ID_TOKEN_REQUEST_TOKEN";

pub struct PublishOptions<'a> {
    pub server: &'a str,
    pub key: Option<&'a str>,
    pub all: bool,
    pub outdated: bool,
    pub project: Option<&'a str>,
    pub dry_run: bool,
}

/// What the console said about one report.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Stored { report_id: i64 },
    Unchanged,
    Refused { status: u16, reason: String },
}

#[derive(Deserialize)]
struct StoredResponse {
    #[serde(rename = "reportId")]
    report_id: i64,
}

#[derive(Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Deserialize)]
struct ErrorBody {
    message: String,
    hint: Option<String>,
}

/// How this run proves who it is.
enum Credential {
    None,
    Key(String),
    GitHubOidc(String),
}

impl Credential {
    fn describe(&self) -> &'static str {
        match self {
            Self::None => "without credentials",
            Self::Key(_) => "with an API key",
            Self::GitHubOidc(_) => "with the GitHub Actions OIDC token",
        }
    }

    fn bearer(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Key(token) | Self::GitHubOidc(token) => Some(token),
        }
    }
}

pub fn cmd_console_publish(repo_root: &Path, options: PublishOptions<'_>) -> Result<()> {
    let projects = if options.all {
        let found = tuff_core::report::find_projects(repo_root)?;
        if found.is_empty() {
            return Err(TuffError::not_found(format!(
                "no tuff.lock under {}",
                repo_root.display()
            ))
            .with_hint("run 'tuff init' in each project folder first"));
        }
        found
    } else {
        vec![repo_root.to_path_buf()]
    };

    let mut reports = Vec::new();
    for project in &projects {
        let outdated = if options.outdated {
            Some(super::outdated::project_outdated_json(project)?)
        } else {
            None
        };
        reports.push(tuff_core::report::build_report(
            project,
            options.project,
            outdated,
        )?);
    }

    if options.dry_run {
        let output = if options.all {
            serde_json::to_string_pretty(&reports)?
        } else {
            serde_json::to_string_pretty(&reports[0])?
        };
        println!("{output}");
        return Ok(());
    }

    let server = normalize_server(options.server)?;
    block_on_oci(async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| {
                TuffError::source_failed(format!("cannot build an HTTP client: {error}"))
            })?;
        let credential = resolve_credential(&client, &server, options.key).await?;
        warn_on_plain_http(&server, &credential);
        println!(
            "Publishing {} to {server} {}.",
            plural(reports.len(), "project"),
            credential.describe()
        );
        let mut refused = 0;
        let mut stored = 0;
        for report in &reports {
            let outcome = send(&client, &server, &credential, report).await?;
            println!("{}", render_outcome(report, &outcome));
            match outcome {
                Outcome::Stored { .. } => stored += 1,
                Outcome::Unchanged => {}
                Outcome::Refused { .. } => refused += 1,
            }
        }
        if refused > 0 {
            return Err(TuffError::refused(format!(
                "the console refused {refused} of {}",
                plural(reports.len(), "project")
            ))
            .with_hint("the reason is listed above for each refused project"));
        }
        let unchanged = reports.len() - stored;
        println!("{stored} stored, {unchanged} unchanged.");
        Ok(())
    })
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// The console's base URL without a trailing slash.
fn normalize_server(server: &str) -> Result<String> {
    let trimmed = server.trim().trim_end_matches('/');
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(
            TuffError::usage(format!("'{server}' is not an http(s) URL")).with_hint(
                "pass --server with the console's address, such as http://127.0.0.1:7474",
            ),
        );
    }
    Ok(trimmed.to_string())
}

/// An API key wins. Without one, a GitHub Actions job with
/// `id-token: write` asks the runner for a token that names the console as
/// its audience. Anywhere else the request goes out unauthenticated, which
/// only a console with nothing configured accepts.
async fn resolve_credential(
    client: &reqwest::Client,
    server: &str,
    key: Option<&str>,
) -> Result<Credential> {
    if let Some(key) = key.filter(|key| !key.is_empty()) {
        return Ok(Credential::Key(key.to_string()));
    }
    let url = std::env::var(OIDC_URL_VAR).ok().filter(|v| !v.is_empty());
    let token = std::env::var(OIDC_TOKEN_VAR).ok().filter(|v| !v.is_empty());
    match (url, token) {
        (Some(url), Some(token)) => Ok(Credential::GitHubOidc(
            request_oidc_token(client, &url, &token, server).await?,
        )),
        _ => Ok(Credential::None),
    }
}

async fn request_oidc_token(
    client: &reqwest::Client,
    request_url: &str,
    request_token: &str,
    audience: &str,
) -> Result<String> {
    #[derive(Deserialize)]
    struct TokenResponse {
        value: String,
    }
    let failed = |reason: String| {
        TuffError::source_failed(format!(
            "cannot get an OIDC token from the GitHub Actions runner: {reason}"
        ))
        .with_hint("the job needs 'permissions: id-token: write', or pass --key")
    };
    let mut url = reqwest::Url::parse(request_url)
        .map_err(|error| failed(format!("{OIDC_URL_VAR} is not a URL: {error}")))?;
    url.query_pairs_mut().append_pair("audience", audience);
    let response = client
        .get(url)
        .bearer_auth(request_token)
        .send()
        .await
        .map_err(|error| failed(error.to_string()))?;
    if !response.status().is_success() {
        return Err(failed(format!("HTTP {}", response.status())));
    }
    let body: TokenResponse = response
        .json()
        .await
        .map_err(|error| failed(format!("unexpected response: {error}")))?;
    Ok(body.value)
}

/// A key or token sent over plain HTTP to another machine can be read on
/// the way.
fn warn_on_plain_http(server: &str, credential: &Credential) {
    if credential.bearer().is_none() {
        return;
    }
    let Some(rest) = server.strip_prefix("http://") else {
        return;
    };
    let host = rest.split(['/', ':']).next().unwrap_or(rest);
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if !(host == "localhost" || host == "::1" || host.starts_with("127.")) {
        eprintln!(
            "warning: {server} is plain HTTP, so the credential can be read on the network. Use an https:// address."
        );
    }
}

async fn send(
    client: &reqwest::Client,
    server: &str,
    credential: &Credential,
    report: &Report,
) -> Result<Outcome> {
    let mut request = client
        .post(format!("{server}/api/v1/reports"))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(serde_json::to_vec(report)?);
    if let Some(token) = credential.bearer() {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.map_err(|error| {
        TuffError::source_failed(format!("cannot reach {server}: {error}")).with_hint(
            "start a console with 'tuff console serve', or pass --server or set TUFF_CONSOLE_URL",
        )
    })?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Ok(interpret(status.as_u16(), &body))
}

fn interpret(status: u16, body: &str) -> Outcome {
    match status {
        201 => match serde_json::from_str::<StoredResponse>(body) {
            Ok(stored) => Outcome::Stored {
                report_id: stored.report_id,
            },
            Err(_) => Outcome::Stored { report_id: 0 },
        },
        200 => Outcome::Unchanged,
        _ => {
            let reason = match serde_json::from_str::<ErrorEnvelope>(body) {
                Ok(envelope) => match envelope.error.hint {
                    Some(hint) => format!("{} ({hint})", envelope.error.message),
                    None => envelope.error.message,
                },
                Err(_) => {
                    let text = body.trim();
                    if text.is_empty() {
                        "no explanation".to_string()
                    } else {
                        text.chars().take(200).collect()
                    }
                }
            };
            Outcome::Refused { status, reason }
        }
    }
}

fn render_outcome(report: &Report, outcome: &Outcome) -> String {
    let project = format!("{} {}", report.project.repository, report.project.path);
    match outcome {
        Outcome::Stored { report_id } if *report_id > 0 => {
            format!("  stored     {project} (report {report_id})")
        }
        Outcome::Stored { .. } => format!("  stored     {project}"),
        Outcome::Unchanged => format!("  unchanged  {project}"),
        Outcome::Refused { status, reason } => {
            format!("  refused    {project}: HTTP {status}, {reason}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_urls_lose_trailing_slashes_and_need_a_scheme() {
        assert_eq!(
            normalize_server("https://tuff.example.dev/").unwrap(),
            "https://tuff.example.dev"
        );
        assert!(normalize_server("tuff.example.dev").is_err());
    }

    #[test]
    fn responses_become_outcomes() {
        assert_eq!(
            interpret(201, r#"{"reportId":7,"projectId":1}"#),
            Outcome::Stored { report_id: 7 }
        );
        assert_eq!(interpret(200, "{}"), Outcome::Unchanged);
        let refused = interpret(
            403,
            r#"{"error":{"kind":"refused","message":"wrong repository","hint":"use its own key"}}"#,
        );
        assert_eq!(
            refused,
            Outcome::Refused {
                status: 403,
                reason: "wrong repository (use its own key)".into()
            }
        );
        assert_eq!(
            interpret(502, "bad gateway"),
            Outcome::Refused {
                status: 502,
                reason: "bad gateway".into()
            }
        );
    }
}
