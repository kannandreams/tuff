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

/// Recorded as `SourceMetadata.source_type` for catalog installs.
pub const SOURCE_TYPE: &str = "catalog";
/// Recorded as `SourceMetadata.url` for catalog installs — there is no
/// remote; the catalog ships inside the binary.
pub const SOURCE_URL: &str = "builtin";

#[derive(Debug, Deserialize)]
struct Catalog {
    catalog_version: String,
    #[serde(default)]
    servers: Vec<CatalogServer>,
}

#[derive(Debug, Deserialize)]
struct CatalogServer {
    id: String,
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

/// The catalog's own version, compared against an installed server's
/// recorded version by `update` and `outdated`.
pub fn version() -> String {
    catalog().catalog_version
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
        metadata: server.tools_summary.map(|tools_summary| McpServerMetadata {
            tools_summary: Some(tools_summary),
        }),
    };
    crate::manifest::validate_mcp_server(&config)
        .map_err(|error| TuffError::new(format!("catalog entry '{id}' is invalid: {error}")))?;

    Ok(Some(CapabilityManifest {
        id: server.id,
        version: catalog.catalog_version,
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
/// export, in a stable order. Used to print a post-install reminder.
pub fn required_env(server: &McpServerConfig) -> Vec<String> {
    server
        .env
        .values()
        .map(|reference| reference.from_env.clone())
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
        assert!(!catalog.catalog_version.is_empty());
        assert!(!catalog.servers.is_empty());
        for id in ids() {
            let manifest = lookup(&id).unwrap().expect("listed id resolves");
            assert_eq!(manifest.id, id);
            assert_eq!(manifest.capability_type, CapabilityType::McpServer);
            assert_eq!(manifest.version, catalog.catalog_version);
            assert!(manifest.server.is_some());
        }
    }

    #[test]
    fn github_entry_references_its_token_by_name_only() {
        let manifest = lookup("github").unwrap().unwrap();
        let server = manifest.server.unwrap();
        assert_eq!(server.transport, McpTransport::Stdio);
        assert_eq!(server.command.as_deref(), Some("npx"));
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
    fn unknown_id_is_none_not_an_error() {
        assert!(lookup("does-not-exist").unwrap().is_none());
    }
}
