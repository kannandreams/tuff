---
title: One MCP server, three harnesses, one command
date: 2026-09-02
authors: kannan
tags: [mcp, tutorial]
excerpt: Declare an external MCP server once, let Tuff write the config entry every harness expects, then prove the server actually starts. A ten-minute walkthrough you can run as you read.
---

Every coding harness reads MCP servers from its own file: Claude Code from `.mcp.json`, Cursor from `.cursor/mcp.json`, Codex and OpenCode from `.agents/mcp.json`. Same server, three dialects, three hand edits per machine, and a typo in any of them fails silently inside the harness. Tuff 0.1.8 makes the server a capability, so the declaration lives in one place and the config entries are generated output.

This walkthrough uses the `everything` server from the built-in catalog. It is the reference server the MCP project publishes for exercising the protocol, it needs no API key, and it makes a good first target for `tuff mcp doctor`. You need `tuff` and Node's `npx` on your `PATH`. Everything below is real output from a fresh directory.

## 1. Install it into every harness at once

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

Two things were written per harness. The config entry is what the harness reads:

```json title=".mcp.json"
{
  "mcpServers": {
    "everything": {
      "args": ["-y", "@modelcontextprotocol/server-everything"],
      "command": "npx"
    }
  }
}
```

The tracked record is the canonical declaration Tuff hashes, so `check` and `diff` treat the server exactly like a skill:

```toml title=".claude/mcp-servers/everything/server.toml"
id = "everything"
version = "1.0.0"
type = "mcp-server"
description = "Reference server exercising the full MCP surface (tools, resources, prompts)."

[server]
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-everything"]

[server.metadata]
tools_summary = "echo, add, longRunningOperation, sampleLLM, getTinyImage"
```

The catalog stores no server code. Each entry is a launch declaration verified against the vendor's own README; the server itself is fetched by `npx`, `uvx`, or `docker` when the harness starts it.

## 2. See it in the lifecycle

```sh frame="terminal"
tuff list
```

```text
│ ID         │ TYPE       │ VERSION │ SCOPE   │ AGENT       │ STATUS  │ PATH                           │
│ everything │ mcp-server │ 1.0.0   │ project │ claude      │ ✓ clean │ .claude/mcp-servers/everything │
│ everything │ mcp-server │ 1.0.0   │ project │ cursor      │ ✓ clean │ .cursor/mcp-servers/everything │
│ everything │ mcp-server │ 1.0.0   │ project │ open-agents │ ✓ clean │ .agents/mcp-servers/everything │
```

One row per harness, because each has its own copy of the entry to keep clean.

## 3. Prove the server starts

A well-formed entry in the right file is not the same as a server that runs. `tuff mcp doctor` spawns each installed server, completes the MCP `initialize` handshake, and asks it for its tool list:

```sh frame="terminal"
tuff mcp doctor
```

```text
│ ID         │ TRANSPORT │ HARNESSES                   │ STATUS │ DETAIL     │
│ everything │ stdio     │ claude, cursor, open-agents │ ✓ ok   │ 13 tool(s) │
```

Thirteen real tools, reported by the real process, in about two seconds. One row rather than three, because the process is the same whichever harness launches it. Doctor exits non-zero when any server is unhealthy, so it sits next to `tuff check` in CI.

## 4. Catch a hand edit

Open `.mcp.json` and append `"--verbose"` to the args, the kind of tweak that happens during debugging and never gets reverted. Tuff notices, because every managed entry carries a baseline hash:

```sh frame="terminal"
tuff check
```

```text
✗ everything               mcp-server claude       modified (.mcp.json#everything)
✓ everything               mcp-server cursor       ok
✓ everything               mcp-server open-agents  ok
```

The failing row names the file and the entry. Restoring the canonical entry is an update, and a plain update refuses to throw away an edit it cannot see the intent of:

```sh frame="terminal"
tuff update everything -a claude
```

```text
error: 'everything' has local changes; run 'tuff diff everything' first or use --force to reload from the catalog
```

```sh frame="terminal"
tuff update everything -a claude --force
tuff check
```

```text
registered MCP server everything (claude) -> .mcp.json
✓ everything               mcp-server claude       ok
```

Servers you added by hand next to Tuff's are never inspected or touched.

## 5. Secrets stay out of the repo

Most useful servers need a token. Tuff never stores one. A manifest can only say which environment variable holds it, and the catalog entry for GitHub's server says exactly that:

```sh frame="terminal"
tuff add mcp github -a claude
tuff mcp doctor
```

```text
note: 'github' reads a variable from the environment; export GITHUB_PERSONAL_ACCESS_TOKEN before starting the harness

│ ID         │ TRANSPORT │ HARNESSES      │ STATUS        │ DETAIL                              │
│ everything │ stdio     │ claude         │ ✓ ok          │ 13 tool(s)                          │
│ github     │ stdio     │ claude         │ ? missing env │ export GITHUB_PERSONAL_ACCESS_TOKEN │
```

The GitHub server was never spawned; doctor checked the environment first and told you what to export. At a real terminal, the install step also asks whether your token lives under a different variable name, so a catalog default never forces you to rename your own environment.

## 6. Clean up

```sh frame="terminal"
tuff delete everything -a claude -a cursor -a open-agents
tuff delete github -a claude
```

The config entries and tracked records go together, and the `mcpServers` object is left in place for whatever you add next.

## What the agent sees

Alongside all of this, Tuff regenerates a small `tuff-capabilities` skill in each harness listing every installed server with its transport and tool summary. The agent reads that on session start, so it knows the `everything` tools are already loaded and can call them directly rather than rediscovering them.

## Where to go next

- The [MCP Servers reference](/primitives/mcp-servers/) covers the manifest, the full catalog, and the safety rules.
- Point `tuff add mcp` at a directory or a git URL to declare a server the catalog does not have.
- If you ship your own server, that is an [MCP-native tool](/primitives/tools/): Tuff copies the code and registers it, rather than pointing at a package.
