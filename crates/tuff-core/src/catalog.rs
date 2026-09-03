//! The built-in MCP server catalog (RFC-102).
//!
//! `tuff add mcp github` resolves against this list instead of a path or git
//! URL. Each entry is turned into an in-memory [`CapabilityManifest`] and
//! installed through exactly the same path as one loaded from `tuff.toml`,
//! so every lifecycle verb works on catalog installs without special cases —
//! the only catalog-aware code is in `update`, `outdated`, and `diff`, which
//! re-resolve here instead of cloning a git repository.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;

use crate::error::{Result, TuffError};
use crate::manifest::{
    CapabilityManifest, CapabilityType, EnvRef, McpServerConfig, McpServerMetadata, McpTransport,
};

const CATALOG_TOML: &str = include_str!("../assets/mcp-catalog.toml");

#[derive(Debug, Deserialize)]
struct Catalog {
    #[serde(default)]
    servers: Vec<CatalogServer>,
}

#[derive(Debug, Deserialize)]
struct CatalogServer {
    id: String,
    /// Independent per entry — bumping one server's version does not mark
    /// every other installed catalog server "outdated" (an earlier, global
    /// `catalog_version` did exactly that).
    version: String,
    description: String,
    #[serde(default)]
    transport: McpTransport,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    url: Option<String>,
    /// Environment variable names the server needs. Each becomes a
    /// `{ from_env = "NAME" }` reference in the generated manifest.
    #[serde(default)]
    env: Vec<String>,
    #[serde(default)]
    tools_summary: Option<String>,
}

fn catalog() -> Catalog {
    toml::from_str(CATALOG_TOML).expect("embedded MCP catalog must parse; covered by unit test")
}

/// Every catalog id, in file order.
pub fn ids() -> Vec<String> {
    catalog().servers.into_iter().map(|s| s.id).collect()
}

/// Resolve a catalog id into a manifest ready for `resolve_capability`.
/// Returns `Ok(None)` for an unknown id so callers can fall through to other
/// source kinds with their own error message.
pub fn lookup(id: &str) -> Result<Option<CapabilityManifest>> {
    let catalog = catalog();
    let Some(server) = catalog.servers.into_iter().find(|s| s.id == id) else {
        return Ok(None);
    };

    let env: BTreeMap<String, EnvRef> = server
        .env
        .into_iter()
        .map(|name| (name.clone(), EnvRef { from_env: name }))
        .collect();
    let config = McpServerConfig {
        transport: server.transport,
        command: server.command,
        args: server.args,
        url: server.url,
        env,
        headers: BTreeMap::new(),
        metadata: server.tools_summary.map(|tools_summary| McpServerMetadata {
            tools_summary: Some(tools_summary),
        }),
    };
    crate::manifest::validate_mcp_server(&config)
        .map_err(|error| TuffError::new(format!("catalog entry '{id}' is invalid: {error}")))?;

    Ok(Some(CapabilityManifest {
        id: server.id,
        version: server.version,
        capability_type: CapabilityType::McpServer,
        description: server.description,
        files: Vec::new(),
        parameters: None,
        implementation: None,
        hook: None,
        workflow: None,
        server: Some(config),
        targets: Vec::new(),
        root: PathBuf::new(),
    }))
}

/// Environment variables a server declaration expects the developer to
/// export, in a stable order, across both `[server.env]` and
/// `[server.headers]`. Used to print a post-install reminder.
pub fn required_env(server: &McpServerConfig) -> Vec<String> {
    server
        .env
        .values()
        .map(|reference| reference.from_env.clone())
        .chain(
            server
                .headers
                .values()
                .map(|reference| reference.from_env.clone()),
        )
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_parses_and_every_entry_validates() {
        let catalog = catalog();
        assert!(!catalog.servers.is_empty());
        for id in ids() {
            let manifest = lookup(&id).unwrap().expect("listed id resolves");
            assert_eq!(manifest.id, id);
            assert_eq!(manifest.capability_type, CapabilityType::McpServer);
            assert!(!manifest.version.is_empty());
            assert!(manifest.server.is_some());
        }
    }

    #[test]
    fn each_entry_versions_independently() {
        // A global version meant bumping one entry marked every installed
        // server "outdated" — confirm the schema really is per-entry.
        let ids = ids();
        assert!(ids.len() >= 2);
        for id in &ids {
            let manifest = lookup(id).unwrap().unwrap();
            assert_eq!(manifest.version, "1.0.0");
        }
    }

    #[test]
    fn github_entry_uses_the_current_docker_based_launch() {
        // The old npm package (@modelcontextprotocol/server-github) was
        // archived upstream in 2025-04; GitHub's own server ships via
        // Docker instead. Pin this so a future edit can't silently regress
        // back to the broken launch command.
        let manifest = lookup("github").unwrap().unwrap();
        let server = manifest.server.unwrap();
        assert_eq!(server.transport, McpTransport::Stdio);
        assert_eq!(server.command.as_deref(), Some("docker"));
        assert!(
            server
                .args
                .iter()
                .any(|arg| arg == "ghcr.io/github/github-mcp-server")
        );
        assert_eq!(
            server.env["GITHUB_PERSONAL_ACCESS_TOKEN"].from_env,
            "GITHUB_PERSONAL_ACCESS_TOKEN"
        );
        assert_eq!(
            required_env(&server),
            vec!["GITHUB_PERSONAL_ACCESS_TOKEN".to_string()]
        );
    }

    #[test]
    fn excluded_servers_are_not_in_the_catalog() {
        // Regression guard for the exclusions documented in the catalog
        // file's header — these package names are confirmed archived or
        // otherwise don't meet the sourcing bar; they should not silently
        // reappear.
        let ids = ids();
        for excluded in [
            "postgres", "sqlite", "slack", "gitlab", "linear", "context7",
        ] {
            assert!(
                !ids.contains(&excluded.to_string()),
                "{excluded} should not be in the catalog"
            );
        }
    }

    #[test]
    fn unknown_id_is_none_not_an_error() {
        assert!(lookup("does-not-exist").unwrap().is_none());
    }
}
