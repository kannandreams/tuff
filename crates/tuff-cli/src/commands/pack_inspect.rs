//! `tuff pack inspect --json` (RFC-110 D7): one document a host can build a
//! review screen from. It says what each member is, what it writes on each
//! harness, whether anything in it runs on its own, which MCP servers it
//! starts or connects to, and the host tables the pack carries.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::adapter::{AdapterKind, AgentAdapter};
use crate::error::Result;
use crate::manifest::{self, CapabilityManifest, CapabilityType, McpTransport};
use crate::pack::{self, PackArtifact, PackAuthor};

/// The JSON document's schema. It changes only by adding fields; a removal
/// or a change of meaning is a new schema number.
pub(crate) const INSPECT_SCHEMA: u8 = 1;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InspectDocument<'a, S: Serialize> {
    schema: u8,
    name: &'a str,
    version: &'a str,
    description: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    homepage: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    license: Option<&'a str>,
    keywords: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<&'a PackAuthor>,
    source: &'a S,
    digest: &'a str,
    artifact_version: u8,
    targets: Vec<&'a str>,
    members: Vec<InspectedMember>,
    /// Parts of a source Tuff could not carry. Empty for a Tuff pack;
    /// filled by imports (RFC-110 D11).
    unsupported: Vec<serde_json::Value>,
    x: &'a tuff_core::host::HostTablesJson,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InspectedMember {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) capability_type: CapabilityType,
    pub(crate) version: String,
    description: String,
    source_path: String,
    /// Whether the harness or the agent runs code from this member: a hook
    /// command, a tool entrypoint, or a local MCP server process.
    pub(crate) runs_code: bool,
    runs_code_reason: &'static str,
    /// Files the member writes on each harness, relative to the project.
    files: BTreeMap<String, Vec<String>>,
    /// Shared harness files the member adds an entry to, such as a hook
    /// settings file or an MCP config, on each harness.
    shared_files: BTreeMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mcp: Option<InspectedMcp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool: Option<InspectedTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hook: Option<InspectedHook>,
    x: tuff_core::host::HostTablesJson,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectedMcp {
    transport: McpTransport,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    /// Names of the environment variables the server reads. Values never
    /// appear in a manifest.
    env: Vec<String>,
    /// Names of the environment variables HTTP headers are read from.
    header_env: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectedTool {
    language: String,
    entrypoint: String,
    /// Whether the tool is served to the harness as an MCP server.
    mcp: bool,
    runtime_deps: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectedHook {
    event: String,
    command: String,
    working_directory: String,
}

/// The inspect document for a verified artifact, from `source`.
pub(crate) fn document<'a, S: Serialize>(
    artifact: &'a PackArtifact,
    source: &'a S,
) -> Result<InspectDocument<'a, S>> {
    let metadata = &artifact.metadata;
    Ok(InspectDocument {
        schema: INSPECT_SCHEMA,
        name: &metadata.name,
        version: &metadata.version,
        description: &metadata.description,
        homepage: metadata.homepage.as_deref(),
        license: metadata.license.as_deref(),
        keywords: &metadata.keywords,
        author: metadata.author.as_ref(),
        source,
        digest: &artifact.digest,
        artifact_version: metadata.artifact_version,
        targets: metadata
            .targets
            .iter()
            .map(|target| target.id.as_str())
            .collect(),
        members: members(artifact)?,
        unsupported: Vec::new(),
        x: &metadata.x,
    })
}

/// Every member of a verified artifact, described from its stored manifest
/// and its rendered targets.
pub(crate) fn members(artifact: &PackArtifact) -> Result<Vec<InspectedMember>> {
    let scratch = tempfile::tempdir()?;
    let sources = scratch.path().join("sources");
    pack::extract_prefix(artifact, "sources", &sources)?;
    let mut members = Vec::new();
    for capability in &artifact.metadata.capabilities {
        let manifest = manifest::load_manifest(&sources.join(&capability.id))?;
        let (runs_code, runs_code_reason) = runs_code(&manifest);
        let mut files = BTreeMap::new();
        let mut shared_files = BTreeMap::new();
        for target in &artifact.metadata.targets {
            let Some(rendered) = target.capabilities.iter().find(|c| c.id == capability.id) else {
                continue;
            };
            let prefix = format!("targets/{}/", target.id);
            let tree = format!("{}/", rendered.installed_path);
            let mut written = artifact
                .contents
                .iter()
                .filter_map(|content| content.path.strip_prefix(&prefix))
                .filter(|path| {
                    path.starts_with(&tree) || rendered.emitted_files.iter().any(|f| f == path)
                })
                .map(str::to_string)
                .collect::<Vec<_>>();
            written.sort();
            written.dedup();
            files.insert(target.id.clone(), written);

            let mut shared = rendered
                .managed_hooks
                .iter()
                .map(|hook| hook.settings_path.clone())
                .collect::<Vec<_>>();
            let registers_mcp = manifest.capability_type == CapabilityType::McpServer
                || manifest
                    .implementation
                    .as_ref()
                    .is_some_and(|tool| tool.mcp);
            if registers_mcp && let Some(adapter) = AdapterKind::from_id(&target.id) {
                shared.push(adapter.mcp_config_relpath().to_string());
            }
            shared.sort();
            shared.dedup();
            shared_files.insert(target.id.clone(), shared);
        }
        members.push(InspectedMember {
            id: capability.id.clone(),
            capability_type: capability.capability_type,
            version: capability.version.clone(),
            description: capability.description.clone(),
            source_path: capability.source_path.clone(),
            runs_code,
            runs_code_reason,
            files,
            shared_files,
            mcp: mcp_of(&manifest),
            tool: manifest.implementation.as_ref().map(|tool| InspectedTool {
                language: tool.language.clone(),
                entrypoint: tool.entrypoint.clone(),
                mcp: tool.mcp,
                runtime_deps: tool.runtime_deps.clone(),
            }),
            hook: manifest.hook.as_ref().map(|hook| InspectedHook {
                event: hook.event.clone(),
                command: hook.command.clone(),
                working_directory: hook.working_directory.clone(),
            }),
            x: tuff_core::host::to_json(&manifest.x),
        });
    }
    Ok(members)
}

/// Whether anything in a member runs without a person choosing to, and why.
/// A skill's folder can hold scripts the agent may decide to run; the files
/// list shows them, and `false` covers only what the harness runs itself.
fn runs_code(manifest: &CapabilityManifest) -> (bool, &'static str) {
    match manifest.capability_type {
        CapabilityType::Hook => (
            true,
            "the harness runs the hook command when its event fires",
        ),
        CapabilityType::Tool => (true, "the agent runs the tool's entrypoint"),
        CapabilityType::McpServer => {
            match manifest.server.as_ref().map(|server| server.transport) {
                Some(McpTransport::Http) => (
                    false,
                    "the harness connects to a remote MCP server over HTTP",
                ),
                _ => (
                    true,
                    "the harness starts the MCP server command as a local process",
                ),
            }
        }
        CapabilityType::Skill => (
            false,
            "instructions and files the agent reads; the harness runs nothing from a skill on its own",
        ),
        CapabilityType::Policy => (false, "permission rules the harness enforces"),
    }
}

fn mcp_of(manifest: &CapabilityManifest) -> Option<InspectedMcp> {
    if let Some(server) = &manifest.server {
        return Some(InspectedMcp {
            transport: server.transport,
            command: server.command.clone(),
            args: server.args.clone(),
            url: server.url.clone(),
            env: server
                .env
                .values()
                .map(|reference| reference.from_env.clone())
                .collect(),
            header_env: server
                .headers
                .values()
                .map(|reference| reference.from_env.clone())
                .collect(),
        });
    }
    let tool = manifest.implementation.as_ref().filter(|tool| tool.mcp)?;
    Some(InspectedMcp {
        transport: McpTransport::Stdio,
        command: Some(tool.language.clone()),
        args: vec![tool.entrypoint.clone()],
        url: None,
        env: Vec::new(),
        header_env: Vec::new(),
    })
}
