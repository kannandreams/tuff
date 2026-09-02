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
//! - **HTTP servers needing headers.** Most remote entries authenticate with
//!   an `Authorization` header, which Tuff's `http` transport cannot express
//!   yet (it is a bare `url`). This is the same gap RFC-102 documented for
//!   `linear` and `context7`.
//! - **Package kinds with no launcher**, such as `mcpb` bundles.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::error::{Result, TuffError};
use crate::manifest::{
    CapabilityManifest, CapabilityType, EnvRef, McpServerConfig, McpServerMetadata, McpTransport,
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
    if let Some(remote) = server.remotes.first() {
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
        metadata: Some(McpServerMetadata {
            tools_summary: None,
        }),
    })
}

fn remote_config(server: &RegistryServer, remote: &RegistryRemote) -> Result<McpServerConfig> {
    if !remote.headers.is_empty() {
        let names: Vec<&str> = remote
            .headers
            .iter()
            .map(|header| header.name.as_str())
            .collect();
        return Err(TuffError::unsupported(format!(
            "registry entry '{}' authenticates with the {} header, which Tuff's http transport cannot express yet",
            server.name,
            names.join(", ")
        )));
    }
    reject_placeholders(server, &remote.url)?;
    Ok(McpServerConfig {
        transport: McpTransport::Http,
        command: None,
        args: Vec::new(),
        url: Some(remote.url.clone()),
        env: BTreeMap::new(),
        metadata: Some(McpServerMetadata {
            tools_summary: None,
        }),
    })
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

    #[test]
    fn a_remote_needing_an_auth_header_is_refused_with_the_header_named() {
        let mut entry = server("a/thing", Vec::new());
        entry.remotes = vec![RegistryRemote {
            transport_type: "streamable-http".into(),
            url: "https://mcp.example.com/v1".into(),
            headers: vec![RegistryVariable {
                name: "Authorization".into(),
                is_required: true,
                description: None,
            }],
        }];
        let error = to_manifest(&entry, "test").unwrap_err();
        assert!(error.to_string().contains("Authorization"), "{error}");
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
