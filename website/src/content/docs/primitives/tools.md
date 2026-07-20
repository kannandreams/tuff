---
title: Tools
description: Tools define executable capabilities an agent can invoke.
---

A tool capability represents an executable capability with a typed parameter contract. Unlike
skills (which inject prose into agent context), tools can change files, call services, or
run commands, and with that power comes stricter validation.

:::note[What is a tool?]
A tool is something the agent can invoke, such as:

- a local binary
- a Python or Node script
- an MCP server
- an HTTP API
- a repository command
- a Docker container
:::

## Manifest

```toml
# coral.toml
id = "security-review"
version = "1.0.0"
type = "tool"
description = "Scan a directory for security vulnerabilities"
files = ["index.js"]                         # optional: entrypoint auto-included

[parameters]
type = "object"
required = ["target_dir"]

[parameters.properties.target_dir]
type = "string"
description = "Directory to scan"

[implementation]
language = "node"
entrypoint = "index.js"
mcp = true                                    # only true for MCP-native tools
runtime_deps = ["chalk", "@octokit/rest"]    # shown at install time, never auto-installed
```

| Field | Required | Description |
|---|---|---|
| `id` | Yes | Stable identifier for the tool |
| `version` | Yes | Semantic version |
| `type` | Yes | Must be `"tool"` |
| `description` | Yes | Shown to the agent: what the tool does and when to call it |
| `files` | No | Source files to copy (entrypoint auto-added if present) |
| `parameters` | Yes | JSON Schema object defining input contract |
| `parameters.type` | Yes | Must be `"object"` |
| `parameters.properties` | Yes | At least one parameter definition required |
| `parameters.required` | No | Array of required parameter names |
| `implementation` | Yes | Execution configuration |
| `implementation.language` | Yes | Runtime language (`node`, `python`, `bash`, etc.) |
| `implementation.entrypoint` | Yes | Relative path to the executable script |
| `implementation.mcp` | No | Set to `true` only when the entrypoint is an MCP stdio server |
| `implementation.runtime_deps` | No | Dependencies displayed at install time |

## Install-time validation

Every tool goes through these checks at `coral add` time:

1. **Schema validation:** `parameters` must be a valid JSON Schema with `type: object` and at least one property
2. **Entrypoint validation:** `entrypoint` must resolve to an existing file, with no path traversal (`../` or absolute paths rejected)
3. **Dependencies displayed:** `runtime_deps` are shown in a note before install; they are never auto-installed
4. **MCP opt-in:** only tools with `implementation.mcp = true` are registered as MCP servers
5. **No execution:** installing a tool only writes files; the entrypoint is never run

## Installing a tool

```sh frame="terminal"
# Local directory (type auto-detected from parent directory)
coral add --agent claude ./my-tool

# Local file with explicit type (subcommand)
coral add tool ./scripts/deploy.sh --agent open-agents

# Git repository
coral add tool https://github.com/owner/repo security-review --agent claude

# Multiple agents
coral add --agent claude --agent open-agents ./my-tool
```

## Example tools

The repository includes example tools under `examples/tools/` that demonstrate
common executable shapes:

| Example | What it demonstrates |
|---|---|
| `local-binary-wrapper` | Wraps approved local binaries such as `git` or `rg` |
| `python-script-tool` | Runs a Python stdlib script with typed parameters |
| `mcp-server-tool` | Provides a minimal stdio MCP server |
| `http-api-tool` | Calls an HTTP endpoint with stdlib networking |
| `repo-command-tool` | Runs an allowlisted repository command |
| `docker-container-tool` | Wraps an allowlisted Docker command |

Install one into the configured default agent:

```sh frame="terminal"
coral add examples/tools/python-script-tool
coral list --type tool
```

Coral records the source, emitted files, and baseline for every tool. MCP
registration is generated only for tools that set `mcp = true`.

## Where files go

| Target | Tool directory | MCP registration |
|---|---|---|
| `open-agents` | `.agents/tools/<id>/` | `.agents/mcp.json` |
| `claude` | `.claude/tools/<id>/` | `.mcp.json` |

For MCP-native tools, set `implementation.mcp = true`. Coral then writes a
launch command pointing at the copied entrypoint and read-merges it into the
harness's native MCP config:

```json
{
  "mcpServers": {
    "security-review": {
      "command": "node",
      "args": [".claude/tools/security-review/index.js"]
    }
  }
}
```

Command-style tools are copied and tracked but are not registered as MCP
servers. Multiple MCP-native tools share a single `mcpServers` object. `coral
delete` cleans up both Coral-generated tool directories and their MCP entries.
`coral untrack` preserves the tool directory and MCP entry while removing Coral
tracking.

## Filtering

```sh frame="terminal"
# Show only tools
coral list --type tool

# Show only skills
coral list --type skill

# Combine with scope filter
coral list --type tool --scope global
```

## Safety

:::caution
Installing a tool grants the harness's agent the ability to execute it later.
Review `implementation.entrypoint` and `runtime_deps` before installing a tool
from a source you don't trust.
:::
