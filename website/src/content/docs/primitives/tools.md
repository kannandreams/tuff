---
title: Tools
description: Tools define executable capabilities an agent can invoke.
---

A tool capability represents an executable capability with a typed parameter contract. Unlike
skills (which inject prose into agent context), tools can change files, call services, or
run commands, and with that power comes stricter validation.

## Manifest

```toml
# coral.toml
id = "security-review"
version = "1.0.0"
primitive = "tool"
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
runtime_deps = ["chalk", "@octokit/rest"]    # shown at install time, never auto-installed
```

| Field | Required | Description |
|---|---|---|
| `id` | Yes | Stable identifier for the tool |
| `version` | Yes | Semantic version |
| `primitive` | Yes | Must be `"tool"` |
| `description` | Yes | Shown to the agent: what the tool does and when to call it |
| `files` | No | Source files to copy (entrypoint auto-added if present) |
| `parameters` | Yes | JSON Schema object defining input contract |
| `parameters.type` | Yes | Must be `"object"` |
| `parameters.properties` | Yes | At least one parameter definition required |
| `parameters.required` | No | Array of required parameter names |
| `implementation` | Yes | Execution configuration |
| `implementation.language` | Yes | Runtime language (`node`, `python`, `bash`, etc.) |
| `implementation.entrypoint` | Yes | Relative path to the executable script |
| `implementation.runtime_deps` | No | Dependencies displayed at install time |

## Install-time validation

Every tool goes through these checks at `coral add` time:

1. **Schema validation:** `parameters` must be a valid JSON Schema with `type: object` and at least one property
2. **Entrypoint validation:** `entrypoint` must resolve to an existing file, with no path traversal (`../` or absolute paths rejected)
3. **Dependencies displayed:** `runtime_deps` are shown in a note before install; they are never auto-installed
4. **No execution:** installing a tool only writes files; the entrypoint is never run

## Installing a tool

```sh frame="terminal"
# Local directory
coral add ./my-tool -a claude

# Git repository (use --tool instead of --skill)
coral add https://github.com/owner/repo --tool security-review -a claude

# Multiple agents
coral add ./my-tool -a claude -a open-agents
```

## Where files go

| Target | Tool directory | MCP registration |
|---|---|---|
| `open-agents` | `.agents/tools/<id>/` | `.agents/mcp.json` |
| `claude` | `.claude/tools/<id>/` | `.mcp.json` |

The MCP config is generated automatically. The adapter reads `coral.toml`, translates parameters,
writes a launch command pointing at the copied entrypoint, and read-merges it into the harness's
native config:

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

Multiple tools share a single `mcpServers` object. `coral delete` cleans up both
Coral-generated tool directories and their MCP entries. `coral untrack` preserves
the tool directory and MCP entry while removing Coral tracking.

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
