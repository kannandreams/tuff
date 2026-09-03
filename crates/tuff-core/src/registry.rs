//! Resolving MCP servers from the official MCP registry.
//!
//! The built-in catalog (`catalog.rs`) is twelve entries compiled into the
//! binary. This module reaches the community registry at
//! <https://registry.modelcontextprotocol.io>, which holds thousands, and
//! turns one of its entries into the same [`CapabilityManifest`] the catalog
//! produces, so installation, drift, update and `mcp doctor` all work on a
//! registry server without knowing where it came from.
//!
//! ## Turning a registry entry into a launch command
//!
//! A registry entry does not carry a command line. It describes a *package*
//! (npm, PyPI, OCI, NuGet) plus the arguments and environment variables that
//! package needs, and leaves assembling the invocation to the client. The
//! rules used here, in order:
//!
//! 1. Prefer a package whose transport is `stdio`; that is what Tuff can
//!    launch and what `tuff mcp doctor` can probe.
//! 2. The command is the entry's `runtimeHint` when it sets one, otherwise
//!    it is derived from `registryType`: `npm` runs under `npx`, `pypi`
//!    under `uvx`, `oci` under `docker`, `nuget` under `dnx`.
//! 3. Arguments are `runtimeArguments`, then the package reference itself,
//!    then `packageArguments`. `npx` also gets `-y` so a first run does not
//!    stop on a prompt, and `docker` gets `run -i --rm` plus one `-e` per
//!    environment variable, because a container sees nothing otherwise.
//! 4. Environment variables contribute their *names* only, as
//!    `{ from_env = "NAME" }` references. A registry entry can carry a
//!    default value for a variable; Tuff never copies one into a manifest,
//!    because a manifest is committed and a value there would be a leaked
//!    secret waiting to happen.
//!
//! ## What is refused, and why refusing is the right answer
//!
//! An entry that cannot be expressed exactly is refused with the reason
//! rather than installed approximately: a wrong launch command wastes more
//! of someone's time than a clear "no".
//!
//! - **Unresolved `{placeholders}`.** The registry lets an argument or URL
//!   carry variables for the client to fill in. Tuff has nowhere to ask, and
//!   guessing a value would produce a command that looks right and fails.
//! - **Literal header values.** A manifest has no field a literal can
//!   occupy, by design, so a header the registry documents as a constant
//!   (`Accept: application/json`) cannot be represented.
//! - **The superseded `sse` transport.** A different handshake from
//!   Streamable HTTP; installing one as the other would write a config no
//!   harness could use.
//! - **Package kinds with no launcher**, such as `mcpb` bundles.
//!
//! ## Remote servers and their headers
//!
//! A remote entry's required headers become `[server.headers]` references
//! (RFC-106). Where the publisher documents the shape of the value, as
//! `Bearer {api_key}`, that becomes the header's `format` and the
//! placeholder's name becomes the variable. Where they document only the
//! header, the variable holds the entire value, prefix included: Tuff does
//! not guess a `Bearer ` that nobody wrote down. Optional headers are left
//! out and named at install time, since requiring a variable the server
//! does not require would report a working server as broken.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::error::{Result, TuffError};
use crate::manifest::{
    CapabilityManifest, CapabilityType, EnvRef, FORMAT_PLACEHOLDER, HeaderRef, McpServerConfig,
    McpServerMetadata, McpTransport,
};

/// The official registry. Overridable so a team can point at their own.
pub const DEFAULT_REGISTRY: &str = "https://registry.modelcontextprotocol.io";

const USER_AGENT: &str = concat!("tuff/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    servers: Vec<ServerEnvelope>,
}

#[derive(Debug, Deserialize)]
struct ServerEnvelope {
    server: RegistryServer,
}

/// One server as the registry describes it. Only the fields Tuff reads.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryServer {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub packages: Vec<RegistryPackage>,
    #[serde(default)]
    pub remotes: Vec<RegistryRemote>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryPackage {
    pub registry_type: String,
    pub identifier: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub runtime_hint: Option<String>,
    #[serde(default)]
    pub transport: Option<RegistryTransport>,
    #[serde(default)]
    pub runtime_arguments: Vec<RegistryArgument>,
    #[serde(default)]
    pub package_arguments: Vec<RegistryArgument>,
    #[serde(default)]
    pub environment_variables: Vec<RegistryVariable>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegistryTransport {
    #[serde(rename = "type", default)]
    pub transport_type: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: Vec<RegistryVariable>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryRemote {
    #[serde(rename = "type", default)]
    pub transport_type: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub headers: Vec<RegistryVariable>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryArgument {
    #[serde(rename = "type", default)]
    pub argument_type: String,
    /// Present on a named argument: the flag, leading dashes included.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryVariable {
    pub name: String,
    #[serde(default)]
    pub is_required: bool,
    #[serde(default)]
    pub description: Option<String>,
    /// On a header, the template the publisher documents, with the secret
    /// standing in as a `{named}` placeholder — `Bearer {api_key}`. Absent
    /// on most headers, meaning the whole value is supplied by the user.
    #[serde(default)]
    pub value: Option<String>,
}

/// Search the registry for the current release of each matching server.
///
/// `version=latest` matters: without it the registry returns every version
/// ever published, so one server appears several times and the first hit is
/// the oldest. Installing a superseded release because it sorted first is
/// exactly the wrong default.
pub async fn search(base_url: &str, query: &str, limit: usize) -> Result<Vec<RegistryServer>> {
    let url = format!(
        "{}/v0/servers?search={}&limit={limit}&version=latest",
        base_url.trim_end_matches('/'),
        urlencode(query)
    );
    let response: SearchResponse = get_json(&url).await?;
    Ok(response
        .servers
        .into_iter()
        .map(|envelope| envelope.server)
        .collect())
}

/// Look one server up by its exact registry name, at its current version.
///
/// The registry has no exact-name endpoint on `v0`, so this searches and
/// then matches the name exactly rather than trusting the first hit: a
/// search for `github` should not silently install someone else's fork.
pub async fn fetch(base_url: &str, name: &str) -> Result<Option<RegistryServer>> {
    let servers = search(base_url, name, 100).await?;
    Ok(servers.into_iter().find(|server| server.name == name))
}

async fn get_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(|error| {
            TuffError::source_failed(format!("could not build the registry HTTP client: {error}"))
        })?;
    let response = client.get(url).send().await.map_err(|error| {
        TuffError::source_failed(format!("could not reach the MCP registry: {error}"))
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(TuffError::source_failed(format!(
            "the MCP registry returned {status} for {url}"
        )));
    }
    let body = response.bytes().await.map_err(|error| {
        TuffError::source_failed(format!("could not read the MCP registry response: {error}"))
    })?;
    serde_json::from_slice(&body).map_err(|error| {
        TuffError::corrupt(format!(
            "the MCP registry returned a response Tuff could not parse: {error}"
        ))
    })
}

/// Percent-encode a query value. Only the characters a search term can
/// realistically contain; the registry accepts the rest verbatim.
fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Segments that identify a protocol rather than a server. `com.notion/mcp`
/// would otherwise install as `mcp`, which says nothing about what it is.
const GENERIC_SEGMENTS: &[&str] = &["mcp", "server", "mcp-server", "server-mcp", "main"];

/// The capability id Tuff installs a registry server under.
///
/// Registry names are reverse-DNS with a path, `io.github.owner/server`, and
/// a capability id must be one path component. The last segment is usually
/// what the publisher would call it, so that is the id; the full name stays
/// recorded in the lockfile as the source.
///
/// When that segment only names the protocol, the publisher's own name is
/// prepended, so `com.notion/mcp` installs as `notion-mcp` rather than the
/// useless `mcp`.
pub fn default_capability_id(name: &str) -> String {
    let (namespace, last) = match name.rsplit_once('/') {
        Some((namespace, last)) => (namespace, last),
        None => ("", name),
    };
    let last = sanitize_segment(last);
    if !GENERIC_SEGMENTS.contains(&last.as_str()) {
        return last;
    }
    let publisher = namespace.rsplit('.').next().unwrap_or_default();
    let publisher = sanitize_segment(publisher);
    if publisher.is_empty() {
        return last;
    }
    if last.is_empty() {
        return publisher;
    }
    format!("{publisher}-{last}")
}

fn sanitize_segment(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    cleaned.trim_matches('-').to_string()
}

/// Build an installable manifest from a registry entry.
///
/// See the module docs for the rules and the deliberate refusals.
pub fn to_manifest(server: &RegistryServer, id: &str) -> Result<CapabilityManifest> {
    let config = server_config(server)?;
    crate::manifest::validate_mcp_server(&config).map_err(|error| {
        TuffError::unsupported(format!(
            "registry entry '{}' does not describe a server Tuff can install: {error}",
            server.name
        ))
    })?;
    Ok(CapabilityManifest {
        id: id.to_string(),
        version: if server.version.is_empty() {
            "0.0.0".to_string()
        } else {
            server.version.clone()
        },
        capability_type: CapabilityType::McpServer,
        description: server.description.clone(),
        files: Vec::new(),
        root: std::path::PathBuf::new(),
        implementation: None,
        parameters: None,
        workflow: None,
        hook: None,
        server: Some(config),
        targets: Vec::new(),
    })
}

fn server_config(server: &RegistryServer) -> Result<McpServerConfig> {
    if let Some(package) = server
        .packages
        .iter()
        .find(|package| package.is_stdio())
        .or_else(|| server.packages.first())
    {
        return stdio_config(server, package);
    }
    if !server.remotes.is_empty() {
        let remote = preferred_remote(server)?;
        return remote_config(server, remote);
    }
    Err(TuffError::unsupported(format!(
        "registry entry '{}' lists no package and no remote endpoint, so there is nothing to launch or connect to",
        server.name
    )))
}

fn stdio_config(server: &RegistryServer, package: &RegistryPackage) -> Result<McpServerConfig> {
    if !package.is_stdio() {
        return Err(TuffError::unsupported(format!(
            "registry entry '{}' only ships a '{}' transport package, which Tuff cannot launch",
            server.name,
            package.transport_type()
        )));
    }
    let command = launch_command(package).ok_or_else(|| {
        TuffError::unsupported(format!(
            "registry entry '{}' is published as '{}', which Tuff has no launcher for",
            server.name, package.registry_type
        ))
    })?;

    let env: BTreeMap<String, EnvRef> = package
        .environment_variables
        .iter()
        .map(|variable| {
            (
                variable.name.clone(),
                EnvRef {
                    from_env: variable.name.clone(),
                },
            )
        })
        .collect();

    let mut args = Vec::new();
    if command == "npx" {
        args.push("-y".to_string());
    }
    if command == "docker" {
        args.extend(["run".to_string(), "-i".to_string(), "--rm".to_string()]);
        for name in env.keys() {
            args.push("-e".to_string());
            args.push(name.clone());
        }
    }
    for argument in &package.runtime_arguments {
        args.extend(render_argument(server, argument)?);
    }
    args.push(package_reference(package));
    for argument in &package.package_arguments {
        args.extend(render_argument(server, argument)?);
    }

    Ok(McpServerConfig {
        transport: McpTransport::Stdio,
        command: Some(command),
        args,
        url: None,
        env,
        // Registry entries that authenticate with a header are refused in
        // `remote_config` until RFC-106 milestone 3 maps them.
        headers: BTreeMap::new(),
        metadata: Some(McpServerMetadata {
            tools_summary: None,
        }),
    })
}

fn remote_config(server: &RegistryServer, remote: &RegistryRemote) -> Result<McpServerConfig> {
    reject_placeholders(server, &remote.url)?;
    let headers = remote_headers(server, remote)?;
    Ok(McpServerConfig {
        transport: McpTransport::Http,
        command: None,
        args: Vec::new(),
        url: Some(remote.url.clone()),
        env: BTreeMap::new(),
        headers,
        metadata: Some(McpServerMetadata {
            tools_summary: None,
        }),
    })
}

/// The remote endpoint Tuff will connect to.
///
/// Entries may list several. `streamable-http` is the transport Tuff speaks
/// and `tuff mcp doctor` probes; `sse` is the superseded 2024-11-05
/// HTTP+SSE transport, which has a different handshake entirely. Preferring
/// the former matters for the 303 entries that publish both, and an entry
/// offering only `sse` is refused rather than installed as though it were
/// Streamable HTTP, which would write a config no harness could use.
fn preferred_remote(server: &RegistryServer) -> Result<&RegistryRemote> {
    if let Some(remote) = server
        .remotes
        .iter()
        .find(|remote| remote.transport_type != SSE_TRANSPORT)
    {
        return Ok(remote);
    }
    Err(TuffError::unsupported(format!(
        "registry entry '{}' offers only the superseded 'sse' transport, which Tuff does not speak",
        server.name
    )))
}

const SSE_TRANSPORT: &str = "sse";

/// Turn a remote's declared headers into `{ from_env = … }` references.
///
/// Measured against every current registry release on 2026-09-03, headers
/// come in three shapes, and each gets a different answer:
///
/// - **No `value` (2,656 of 3,072).** The publisher documents a header but
///   not how to build it, so the variable holds the whole header value,
///   prefix and all. Tuff writes no `format`: inventing `Bearer ` for an
///   `Authorization` header would be right often and wrong silently, and a
///   wrong guess produces a config that looks correct and fails inside the
///   agent.
/// - **A `value` with exactly one `{placeholder}` (403).** The publisher
///   said how to build it, so that becomes the `format` and the
///   placeholder's own name becomes the variable.
/// - **A `value` with no placeholder (13).** A literal, such as
///   `Accept: application/json`. There is deliberately no field in a Tuff
///   manifest a literal header value can occupy, so these are refused.
///
/// Optional headers are left out. Over a thousand entries declare one, and
/// requiring a variable the server does not require would report every one
/// of them as `missing env`. The caller names what was skipped so the
/// choice is visible rather than silent.
fn remote_headers(
    server: &RegistryServer,
    remote: &RegistryRemote,
) -> Result<BTreeMap<String, HeaderRef>> {
    let capability_id = default_capability_id(&server.name);
    let mut headers = BTreeMap::new();

    for header in remote.headers.iter().filter(|header| header.is_required) {
        let name = header.name.trim();
        if name.is_empty() {
            return Err(TuffError::unsupported(format!(
                "registry entry '{}' declares a header with no name",
                server.name
            )));
        }

        let reference = match header.value.as_deref() {
            Some(value) => header_from_template(server, name, value, &capability_id)?,
            None => HeaderRef {
                from_env: sanitized_env_name(&format!("{capability_id}_{name}")),
                format: None,
            },
        };
        headers.insert(name.to_string(), reference);
    }

    Ok(headers)
}

/// Split a documented template such as `Bearer {api_key}` into the format
/// Tuff records and the variable that fills it.
fn header_from_template(
    server: &RegistryServer,
    header: &str,
    value: &str,
    capability_id: &str,
) -> Result<HeaderRef> {
    let placeholders = placeholder_names(value);
    let [placeholder] = placeholders.as_slice() else {
        let reason = if placeholders.is_empty() {
            "a literal value, and a Tuff manifest has no field a literal header value can occupy"
        } else {
            "a value built from more than one variable, which Tuff cannot express"
        };
        return Err(TuffError::unsupported(format!(
            "registry entry '{}' declares the {header} header with {reason}",
            server.name
        )));
    };

    // A publisher-chosen name like `smithery_api_key` already says which
    // service it belongs to. A generic one does not, and two servers both
    // wanting `API_KEY` would quietly share a variable.
    let qualified = if GENERIC_VARIABLE_NAMES.contains(&placeholder.to_ascii_lowercase().as_str()) {
        format!("{capability_id}_{placeholder}")
    } else {
        placeholder.clone()
    };

    Ok(HeaderRef {
        from_env: sanitized_env_name(&qualified),
        format: Some(value.replacen(&format!("{{{placeholder}}}"), FORMAT_PLACEHOLDER, 1)),
    })
}

/// Placeholder names in a header template, in order of appearance.
fn placeholder_names(value: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = value;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        names.push(rest[open + 1..open + close].to_string());
        rest = &rest[open + close + 1..];
    }
    names
}

/// Placeholder names that say nothing about which service they belong to.
const GENERIC_VARIABLE_NAMES: &[&str] = &["api_key", "apikey", "token", "key", "secret", "auth"];

/// A name legal in a shell environment: upper case, underscores only, no
/// leading digit.
fn sanitized_env_name(raw: &str) -> String {
    let mut name = String::with_capacity(raw.len());
    for character in raw.chars() {
        if character.is_ascii_alphanumeric() {
            name.push(character.to_ascii_uppercase());
        } else if !name.ends_with('_') {
            name.push('_');
        }
    }
    let name = name.trim_matches('_').to_string();
    if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("_{name}")
    } else {
        name
    }
}

/// Headers a registry entry declares but Tuff leaves out of the manifest,
/// because the server does not require them. Named at install time so the
/// omission is visible and can be added by hand.
pub fn skipped_optional_headers(server: &RegistryServer) -> Vec<String> {
    // A package always wins over a remote, so an entry shipping one never
    // reaches the header path at all.
    if !server.packages.is_empty() {
        return Vec::new();
    }
    let Ok(remote) = preferred_remote(server) else {
        return Vec::new();
    };
    let mut names: Vec<String> = remote
        .headers
        .iter()
        .filter(|header| !header.is_required)
        .map(|header| header.name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// `npx`/`uvx`/`docker`/`dnx`, from the entry's hint or its package kind.
fn launch_command(package: &RegistryPackage) -> Option<String> {
    if let Some(hint) = package
        .runtime_hint
        .as_deref()
        .map(str::trim)
        .filter(|hint| !hint.is_empty())
    {
        return Some(hint.to_string());
    }
    match package.registry_type.as_str() {
        "npm" => Some("npx".to_string()),
        "pypi" => Some("uvx".to_string()),
        "oci" | "docker" => Some("docker".to_string()),
        "nuget" => Some("dnx".to_string()),
        _ => None,
    }
}

/// How the package is named on its own command line.
fn package_reference(package: &RegistryPackage) -> String {
    if package.version.is_empty() {
        return package.identifier.clone();
    }
    // An image is `name:tag`; every other ecosystem Tuff launches uses `@`.
    let separator = if matches!(package.registry_type.as_str(), "oci" | "docker") {
        ':'
    } else {
        '@'
    };
    format!("{}{separator}{}", package.identifier, package.version)
}

fn render_argument(server: &RegistryServer, argument: &RegistryArgument) -> Result<Vec<String>> {
    let value = argument
        .value
        .as_deref()
        .or(argument.default.as_deref())
        .unwrap_or_default();
    reject_placeholders(server, value)?;
    if let Some(name) = argument.name.as_deref() {
        reject_placeholders(server, name)?;
        if value.is_empty() {
            return Ok(vec![name.to_string()]);
        }
        return Ok(vec![name.to_string(), value.to_string()]);
    }
    if value.is_empty() {
        return Err(TuffError::unsupported(format!(
            "registry entry '{}' has a positional argument with no value, so Tuff cannot build its command line",
            server.name
        )));
    }
    Ok(vec![value.to_string()])
}

/// A `{placeholder}` means the registry expects the client to substitute a
/// value. Tuff has nowhere to ask for one, and a guessed value produces a
/// command that looks right and fails at launch.
fn reject_placeholders(server: &RegistryServer, value: &str) -> Result<()> {
    if value.contains('{') && value.contains('}') {
        return Err(TuffError::unsupported(format!(
            "registry entry '{}' needs a value substituted into '{value}', which Tuff cannot supply",
            server.name
        ))
        .with_hint("install it from a local manifest with the value filled in"));
    }
    Ok(())
}

impl RegistryPackage {
    fn transport_type(&self) -> &str {
        self.transport
            .as_ref()
            .map(|transport| transport.transport_type.as_str())
            .unwrap_or("stdio")
    }

    fn is_stdio(&self) -> bool {
        self.transport_type() == "stdio"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn npm_package(identifier: &str, version: &str) -> RegistryPackage {
        RegistryPackage {
            registry_type: "npm".into(),
            identifier: identifier.into(),
            version: version.into(),
            runtime_hint: None,
            transport: Some(RegistryTransport {
                transport_type: "stdio".into(),
                url: None,
                headers: Vec::new(),
            }),
            runtime_arguments: Vec::new(),
            package_arguments: Vec::new(),
            environment_variables: Vec::new(),
        }
    }

    fn server(name: &str, packages: Vec<RegistryPackage>) -> RegistryServer {
        RegistryServer {
            name: name.into(),
            description: "A test server.".into(),
            version: "1.2.3".into(),
            packages,
            remotes: Vec::new(),
        }
    }

    fn config_of(server: &RegistryServer) -> McpServerConfig {
        to_manifest(server, "test").unwrap().server.unwrap()
    }

    #[test]
    fn an_npm_package_becomes_an_npx_command_pinned_to_its_version() {
        let config = config_of(&server(
            "io.github.acme/thing",
            vec![npm_package("thing", "1.2.3")],
        ));
        assert_eq!(config.command.as_deref(), Some("npx"));
        // -y so a first run does not stop at a prompt inside the harness.
        assert_eq!(config.args, vec!["-y", "thing@1.2.3"]);
        assert_eq!(config.transport, McpTransport::Stdio);
    }

    #[test]
    fn the_command_comes_from_the_package_kind_when_no_hint_is_given() {
        // Most registry entries omit runtimeHint, so deriving the launcher
        // from registryType is the common path, not the fallback.
        let mut pypi = npm_package("thing", "1.0.0");
        pypi.registry_type = "pypi".into();
        assert_eq!(
            config_of(&server("a/thing", vec![pypi])).command.as_deref(),
            Some("uvx")
        );

        let mut nuget = npm_package("thing", "1.0.0");
        nuget.registry_type = "nuget".into();
        assert_eq!(
            config_of(&server("a/thing", vec![nuget]))
                .command
                .as_deref(),
            Some("dnx")
        );
    }

    #[test]
    fn a_runtime_hint_wins_over_the_package_kind() {
        let mut package = npm_package("thing", "1.0.0");
        package.runtime_hint = Some("bunx".into());
        assert_eq!(
            config_of(&server("a/thing", vec![package]))
                .command
                .as_deref(),
            Some("bunx")
        );
    }

    #[test]
    fn an_image_is_run_with_docker_and_told_about_its_environment() {
        // A container sees no environment unless each variable is passed
        // through explicitly, so the -e flags are not optional.
        let mut package = npm_package("ghcr.io/acme/thing", "2.0.0");
        package.registry_type = "oci".into();
        package.environment_variables = vec![RegistryVariable {
            name: "API_TOKEN".into(),
            is_required: true,
            description: None,
            value: None,
        }];
        let config = config_of(&server("a/thing", vec![package]));
        assert_eq!(config.command.as_deref(), Some("docker"));
        assert_eq!(
            config.args,
            vec![
                "run",
                "-i",
                "--rm",
                "-e",
                "API_TOKEN",
                "ghcr.io/acme/thing:2.0.0"
            ]
        );
    }

    #[test]
    fn environment_variables_become_references_never_values() {
        let mut package = npm_package("thing", "1.0.0");
        package.environment_variables = vec![RegistryVariable {
            name: "API_TOKEN".into(),
            is_required: true,
            description: Some("A token.".into()),
            value: None,
        }];
        let config = config_of(&server("a/thing", vec![package]));
        assert_eq!(config.env["API_TOKEN"].from_env, "API_TOKEN");
    }

    #[test]
    fn arguments_are_ordered_runtime_then_package_then_package_arguments() {
        let mut package = npm_package("thing", "1.0.0");
        package.runtime_arguments = vec![RegistryArgument {
            argument_type: "positional".into(),
            name: None,
            value: Some("--quiet".into()),
            default: None,
        }];
        package.package_arguments = vec![
            RegistryArgument {
                argument_type: "positional".into(),
                name: None,
                value: Some("serve".into()),
                default: None,
            },
            RegistryArgument {
                argument_type: "named".into(),
                name: Some("--port".into()),
                value: Some("8080".into()),
                default: None,
            },
        ];
        let config = config_of(&server("a/thing", vec![package]));
        assert_eq!(
            config.args,
            vec!["-y", "--quiet", "thing@1.0.0", "serve", "--port", "8080"]
        );
    }

    #[test]
    fn an_entry_needing_a_substituted_value_is_refused_rather_than_guessed() {
        // The registry lets a publisher leave {placeholders} for the client
        // to fill in. A guessed value builds a command that looks right and
        // fails at launch, which is worse than saying no.
        let mut package = npm_package("thing", "1.0.0");
        package.package_arguments = vec![RegistryArgument {
            argument_type: "positional".into(),
            name: None,
            value: Some("{directory}".into()),
            default: None,
        }];
        let error = to_manifest(&server("a/thing", vec![package]), "test").unwrap_err();
        assert_eq!(error.kind(), crate::error::ErrorKind::Unsupported);
        assert!(error.to_string().contains("{directory}"), "{error}");
        assert!(error.hint().is_some());
    }

    fn header(name: &str, is_required: bool, value: Option<&str>) -> RegistryVariable {
        RegistryVariable {
            name: name.into(),
            is_required,
            description: None,
            value: value.map(str::to_string),
        }
    }

    fn remote_entry(name: &str, headers: Vec<RegistryVariable>) -> RegistryServer {
        let mut entry = server(name, Vec::new());
        entry.remotes = vec![RegistryRemote {
            transport_type: "streamable-http".into(),
            url: "https://mcp.example.com/v1".into(),
            headers,
        }];
        entry
    }

    /// The common shape by a wide margin: the publisher names the header
    /// and says nothing about how to build its value.
    #[test]
    fn a_header_without_a_documented_value_takes_the_whole_value_from_one_variable() {
        let entry = remote_entry("a/thing", vec![header("Authorization", true, None)]);
        let config = to_manifest(&entry, "thing").unwrap().server.unwrap();

        let reference = &config.headers["Authorization"];
        assert_eq!(reference.from_env, "THING_AUTHORIZATION");
        // No invented `Bearer `. The variable holds the entire header
        // value, prefix included, because nobody wrote down which prefix.
        assert_eq!(reference.format, None);
    }

    #[test]
    fn a_documented_template_becomes_the_format_and_names_the_variable() {
        let entry = remote_entry(
            "ai.smithery/notion",
            vec![header(
                "Authorization",
                true,
                Some("Bearer {smithery_api_key}"),
            )],
        );
        let config = to_manifest(&entry, "notion").unwrap().server.unwrap();

        let reference = &config.headers["Authorization"];
        assert_eq!(reference.from_env, "SMITHERY_API_KEY");
        assert_eq!(reference.format.as_deref(), Some("Bearer {}"));
        assert_eq!(reference.render("secret"), "Bearer secret");
    }

    /// `{api_key}` says nothing about whose key it is, so two servers would
    /// quietly share one variable.
    #[test]
    fn a_generic_placeholder_name_is_qualified_by_the_capability() {
        let entry = remote_entry(
            "ai.bowmark/bowmark",
            vec![header("Authorization", true, Some("Bearer {api_key}"))],
        );
        let config = to_manifest(&entry, "bowmark").unwrap().server.unwrap();
        assert_eq!(config.headers["Authorization"].from_env, "BOWMARK_API_KEY");
    }

    #[test]
    fn a_literal_header_value_is_refused_rather_than_smuggled_into_the_manifest() {
        let entry = remote_entry(
            "a/thing",
            vec![header("Accept", true, Some("application/json"))],
        );
        let error = to_manifest(&entry, "thing").unwrap_err();
        assert_eq!(error.kind(), crate::error::ErrorKind::Unsupported);
        assert!(error.to_string().contains("Accept"), "{error}");
        assert!(error.to_string().contains("literal"), "{error}");
    }

    #[test]
    fn a_header_built_from_two_variables_is_refused() {
        let entry = remote_entry(
            "a/thing",
            vec![header("Authorization", true, Some("{scheme} {token}"))],
        );
        let error = to_manifest(&entry, "thing").unwrap_err();
        assert!(error.to_string().contains("more than one"), "{error}");
    }

    /// Over a thousand entries declare an optional header. Requiring one
    /// would report a working server as `missing env`.
    #[test]
    fn optional_headers_are_left_out_and_named_instead() {
        let entry = remote_entry(
            "a/thing",
            vec![
                header("Authorization", true, None),
                header("X-Request-Id", false, None),
            ],
        );
        let config = to_manifest(&entry, "thing").unwrap().server.unwrap();

        assert!(config.headers.contains_key("Authorization"));
        assert!(!config.headers.contains_key("X-Request-Id"));
        assert_eq!(skipped_optional_headers(&entry), vec!["X-Request-Id"]);
    }

    /// `sse` is the superseded transport with a different handshake, not a
    /// dialect of Streamable HTTP.
    #[test]
    fn streamable_http_is_preferred_and_an_sse_only_entry_is_refused() {
        let mut entry = remote_entry("a/thing", Vec::new());
        entry.remotes.insert(
            0,
            RegistryRemote {
                transport_type: "sse".into(),
                url: "https://mcp.example.com/sse".into(),
                headers: Vec::new(),
            },
        );
        let config = to_manifest(&entry, "thing").unwrap().server.unwrap();
        assert_eq!(config.url.as_deref(), Some("https://mcp.example.com/v1"));

        entry.remotes.truncate(1);
        let error = to_manifest(&entry, "thing").unwrap_err();
        assert!(error.to_string().contains("sse"), "{error}");
    }

    #[test]
    fn a_variable_name_is_made_legal_for_a_shell() {
        assert_eq!(sanitized_env_name("smithery-api.key"), "SMITHERY_API_KEY");
        assert_eq!(sanitized_env_name("thing_X-Api-Key"), "THING_X_API_KEY");
        assert_eq!(sanitized_env_name("_leading_"), "LEADING");
        assert_eq!(sanitized_env_name("2fa token"), "_2FA_TOKEN");
    }

    #[test]
    fn a_remote_without_headers_installs_as_an_http_server() {
        let mut entry = server("a/thing", Vec::new());
        entry.remotes = vec![RegistryRemote {
            transport_type: "streamable-http".into(),
            url: "https://mcp.example.com/v1".into(),
            headers: Vec::new(),
        }];
        let config = config_of(&entry);
        assert_eq!(config.transport, McpTransport::Http);
        assert_eq!(config.url.as_deref(), Some("https://mcp.example.com/v1"));
    }

    #[test]
    fn a_package_kind_with_no_launcher_is_refused() {
        let mut package = npm_package("thing", "1.0.0");
        package.registry_type = "mcpb".into();
        let error = to_manifest(&server("a/thing", vec![package]), "test").unwrap_err();
        assert!(error.to_string().contains("mcpb"), "{error}");
    }

    #[test]
    fn an_entry_with_nothing_to_launch_is_refused() {
        let error = to_manifest(&server("a/thing", Vec::new()), "test").unwrap_err();
        assert!(
            error.to_string().contains("no package and no remote"),
            "{error}"
        );
    }

    #[test]
    fn a_capability_id_keeps_the_publishers_name_when_the_last_segment_is_generic() {
        assert_eq!(
            default_capability_id("io.github.domdomegg/filesystem-mcp"),
            "filesystem-mcp"
        );
        // "com.notion/mcp" as "mcp" would say nothing about what it is.
        assert_eq!(default_capability_id("com.notion/mcp"), "notion-mcp");
        assert_eq!(
            default_capability_id("ai.smithery/server"),
            "smithery-server"
        );
        assert_eq!(default_capability_id("plain-name"), "plain-name");
    }

    #[test]
    fn a_capability_id_never_contains_a_path_separator() {
        // `tuff add` rejects an id with a slash, so the derivation has to
        // produce one component whatever the registry name looks like.
        for name in [
            "io.github.owner/deep/nested/name",
            "weird name/with spaces",
            "a/b",
        ] {
            let id = default_capability_id(name);
            assert!(!id.contains('/'), "{name} produced {id}");
            assert!(!id.is_empty(), "{name} produced an empty id");
        }
    }
}
