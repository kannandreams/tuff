---
title: Managing MCP server configuration across coding agents
description: Declare an MCP server once, and Tuff writes the config for Claude Code, Cursor, and Codex and checks that the server starts.
date: 2026-09-02
authors: kannan
tags: [mcp, tutorial]
excerpt: Declare an external MCP server once, and Tuff writes the config entry each harness reads and checks that the server starts. The steps run in a new directory in about ten minutes.
---

Each coding harness reads MCP servers from its own file: Claude Code from `.mcp.json`, Cursor from `.cursor/mcp.json`, and Codex and OpenCode from `.agents/mcp.json`. The same server needs one hand edit per file, and the harness does not report a typo in any of them. Since Tuff 0.1.8 an MCP server is a capability: the declaration lives in one place, and Tuff generates the config entries.

This walkthrough uses the `everything` server from the built-in catalog. It is the reference server the MCP project publishes for exercising the protocol, and it needs no API key. You need `tuff` and Node's `npx` on your `PATH`.

## 1. Install the server for three harnesses

```sh frame="terminal"
mkdir mcp-demo && cd mcp-demo
tuff init
tuff add mcp everything -a claude -a cursor -a open-agents
```

```text
installed everything (claude) -> .claude/mcp-servers/everything/server.toml
installed everything (cursor) -> .cursor/mcp-servers/everything/server.toml
installed everything (open-agents) -> .agents/mcp-servers/everything/server.toml
registered MCP server everything (claude) -> .mcp.json
registered MCP server everything (cursor) -> .cursor/mcp.json
registered MCP server everything (open-agents) -> .agents/mcp.json
installed everything from the built-in catalog (catalog 1.0.0)
```

Tuff writes two files per harness. The config entry is the file the harness reads:

```json title=".mcp.json"
{
  "mcpServers": {
    "everything": {
      "args": [
        "-y",
        "@modelcontextprotocol/server-everything"
      ],
      "command": "npx"
    }
  }
}
```

The tracked record is the declaration Tuff hashes. `tuff check` and `tuff diff` use it in the same way as for a skill:

```toml title=".claude/mcp-servers/everything/server.toml"
id = "everything"
version = "1.0.0"
type = "mcp-server"
description = "Reference server exercising the full MCP surface: tools, resources, and prompts. Needs no API key, which makes it a good first target for tuff mcp doctor."

[server]
transport = "stdio"
command = "npx"
args = [
    "-y",
    "@modelcontextprotocol/server-everything",
]

[server.env]

[server.metadata]
tools_summary = "echo, add, longRunningOperation, sampleLLM, getTinyImage"
```

Each catalog entry is a launch declaration checked against the vendor's README. The catalog holds no server code: `npx`, `uvx`, or `docker` fetches the server when the harness starts it.

## 2. List the installed server

```sh frame="terminal"
tuff list
```

```text
┌───────────────────┬────────────┬─────────┬─────────┬─────────────┬─────────┬──────────────────────────────────┐
│ ID                │ TYPE       │ VERSION │ SCOPE   │ AGENT       │ STATUS  │ PATH                             │
├───────────────────┼────────────┼─────────┼─────────┼─────────────┼─────────┼──────────────────────────────────┤
│ everything        │ mcp-server │ 1.0.0   │ project │ claude      │ ✓ clean │ .claude/mcp-servers/everything   │
│ everything        │ mcp-server │ 1.0.0   │ project │ cursor      │ ✓ clean │ .cursor/mcp-servers/everything   │
│ everything        │ mcp-server │ 1.0.0   │ project │ open-agents │ ✓ clean │ .agents/mcp-servers/everything   │
│ tuff-capabilities │ skill      │ 1.0.0   │ project │ open-agents │ ✓ clean │ .agents/skills/tuff-capabilities │
│ tuff-cli-guide    │ skill      │ 0.1.0   │ project │ open-agents │ ✓ clean │ .agents/skills/tuff-cli-guide    │
└───────────────────┴────────────┴─────────┴─────────┴─────────────┴─────────┴──────────────────────────────────┘
```

`tuff list` shows one `everything` row per harness, because each harness has its own copy of the entry. `tuff init` installed the `tuff-cli-guide` skill, and the `tuff-capabilities` skill is described in the last section.

## 3. Check that the server starts

`tuff mcp doctor` starts each installed server, completes the MCP `initialize` handshake, and requests its tool list:

```sh frame="terminal"
tuff mcp doctor
```

```text
┌────────────┬───────────┬─────────────────────────────┬────────┬────────────┐
│ ID         │ TRANSPORT │ HARNESSES                   │ STATUS │ DETAIL     │
├────────────┼───────────┼─────────────────────────────┼────────┼────────────┤
│ everything │ stdio     │ claude, cursor, open-agents │ ✓ ok   │ 13 tool(s) │
└────────────┴───────────┴─────────────────────────────┴────────┴────────────┘
```

The server reported 13 tools. With the npm package already cached, the check took about half a second. Doctor prints one row per server, because every harness launches the same process. It exits non-zero when a server is unhealthy, so it can run in CI next to `tuff check`.

## 4. Detect a hand edit

Add `"--verbose"` to the args in `.mcp.json`. Every managed entry has a baseline hash, so `tuff check` reports the change:

```sh frame="terminal"
tuff check
```

```text
✗ everything               mcp-server claude       modified (.mcp.json#everything)
✓ everything               mcp-server cursor       ok
✓ everything               mcp-server open-agents  ok
✓ tuff-capabilities        skill open-agents  ok
✓ tuff-cli-guide           skill open-agents  ok
```

The failing row names the file and the entry. `tuff update` restores the entry, and without `--force` it refuses to overwrite a local change:

```sh frame="terminal"
tuff update everything -a claude
```

```text
error: 'everything' has local changes
hint: run 'tuff diff everything' first, or use --force to reload from the catalog
```

```sh frame="terminal"
tuff update everything -a claude --force
tuff check
```

```text
installed everything (claude) -> .claude/mcp-servers/everything/server.toml
registered MCP server everything (claude) -> .mcp.json
✓ everything               mcp-server claude       ok
✓ everything               mcp-server cursor       ok
✓ everything               mcp-server open-agents  ok
✓ tuff-capabilities        skill open-agents  ok
✓ tuff-cli-guide           skill open-agents  ok
```

`tuff check` and `tuff update` leave servers added by hand next to Tuff's unchanged.

## 5. Servers that need a token

A manifest names the environment variable that holds a token, and Tuff does not store the token. The catalog entry for GitHub's server uses `GITHUB_PERSONAL_ACCESS_TOKEN`:

```sh frame="terminal"
tuff add mcp github -a claude
tuff mcp doctor
```

```text
installed github (claude) -> .claude/mcp-servers/github/server.toml
registered MCP server github (claude) -> .mcp.json
note: 'github' reads a variable from the environment; export GITHUB_PERSONAL_ACCESS_TOKEN before starting the harness
installed github from the built-in catalog (catalog 1.0.0)
┌────────────┬───────────┬─────────────────────────────┬───────────────┬─────────────────────────────────────┐
│ ID         │ TRANSPORT │ HARNESSES                   │ STATUS        │ DETAIL                              │
├────────────┼───────────┼─────────────────────────────┼───────────────┼─────────────────────────────────────┤
│ everything │ stdio     │ claude, cursor, open-agents │ ✓ ok          │ 13 tool(s)                          │
│ github     │ stdio     │ claude                      │ ? missing env │ export GITHUB_PERSONAL_ACCESS_TOKEN │
└────────────┴───────────┴─────────────────────────────┴───────────────┴─────────────────────────────────────┘
```

Doctor checks the environment before starting a server, so the GitHub server was not started. In an interactive terminal, `tuff add` also asks whether the token is stored under a different variable name.

## 6. Remove the servers

```sh frame="terminal"
tuff delete everything -a claude -a cursor -a open-agents
tuff delete github -a claude
```

```text
deleted 'everything' from project scope
deleted 'github' from project scope
```

`tuff delete` removes the config entries and the tracked records, and leaves the `mcpServers` object in each file.

## The tuff-capabilities skill

Tuff regenerates a `tuff-capabilities` skill in `.agents/skills/` when capabilities change. It lists every installed server with its description, transport, and tool summary, and tells the agent that the servers are already loaded by the harness.

## Further reading

- The [MCP Servers reference](/primitives/mcp-servers/) covers the manifest, the full catalog, and the safety rules.
- `tuff add mcp` also accepts a directory or a git URL for a server the catalog does not include.
- A server you ship yourself is an [MCP-native tool](/primitives/tools/): Tuff copies its code and registers it.
