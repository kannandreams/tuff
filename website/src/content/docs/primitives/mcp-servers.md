---
title: MCP Servers
description: Declare an external MCP server once and wire it into every harness's native MCP config.
---

An `mcp-server` capability is an **external** MCP server — one whose code you
do not ship, only point at: the GitHub server, a filesystem server, something a
teammate published. Tuff turns one declaration into the right entry in each
harness's MCP config, then tracks it like any other capability: listed,
checked for drift, updatable, deletable.

:::note[MCP server vs. MCP-native tool]
Two different things carry "MCP" in this documentation:

- **MCP server** (`type = "mcp-server"`, this page) — an external server.
  Tuff writes the config entry that launches or connects to it.
- **MCP-native tool** (`type = "tool"` with `implementation.mcp = true`, see
  [Tools](/primitives/tools)) — a server whose code Tuff copies into the
  project and registers. Use this when the server is *yours*.
:::

## Why a separate kind

Wiring one server by hand means editing `.mcp.json`, `.cursor/mcp.json`, and
`.agents/mcp.json` in three dialects, repeating that per machine, and inventing
a convention for where the token comes from. A typo fails silently inside the
harness. `mcp-server` makes the declaration the source of truth and the
config entries generated output.

## Manifest

```toml
id = "github"
version = "1.0.0"
type = "mcp-server"
description = "GitHub MCP server: issues, pull requests, and code search."

[server]
transport = "stdio"          # "stdio" (default) or "http"
command = "npx"              # required for stdio
args = ["-y", "@modelcontextprotocol/server-github"]
# url = "https://…"          # required for http

[server.env]
GITHUB_PERSONAL_ACCESS_TOKEN = { from_env = "GITHUB_PERSONAL_ACCESS_TOKEN" }

[server.metadata]            # optional
tools_summary = "create_issue, create_pull_request, list_issues, search_code"
```

| Field | Meaning | Required |
|---|---|---|
| `server.transport` | `stdio` or `http` | No (defaults to `stdio`) |
| `server.command` / `server.args` | Process to launch for `stdio` | `command` for stdio |
| `server.url` | Endpoint for `http` | For http |
| `server.env` | Environment the harness passes to the server, as **references** | No |
| `server.metadata.tools_summary` | One-line list of what the server exposes; surfaces in the generated capability index | No |

### Secrets are references, never values

Every `[server.env]` value must be `{ from_env = "NAME" }`. There is no field a
literal secret can legally occupy, so the manifest can be committed and shared.
A bare string is rejected at install time with an error that names the
`from_env` form. After install, Tuff prints which variables you still need to
export.

## Installing

From the built-in catalog, a local directory, or a git URL — several at once:

```sh frame="terminal"
tuff add mcp github filesystem -a claude -a cursor -a open-agents
tuff add mcp ./mcp-servers/internal-search
tuff add mcp https://github.com/acme/capabilities//mcp-servers/linear
```

The catalog currently ships `github`, `filesystem`, and `memory`. Git sources
must name the subdirectory holding `tuff.toml`.

Every lifecycle verb works unchanged:

```sh frame="terminal"
tuff list --type mcp-server
tuff check
tuff outdated          # catalog installs compare against the catalog version
tuff update github
tuff delete github     # removes the config entry and the tracked record
```

## What gets written

For each selected harness Tuff writes two things:

| Harness | Config entry | Env reference syntax | Tracked record |
|---|---|---|---|
| `claude` | `.mcp.json` | `${VAR}` | `.claude/mcp-servers/<id>/server.toml` |
| `cursor` | `.cursor/mcp.json` | `${env:VAR}` | `.cursor/mcp-servers/<id>/server.toml` |
| `open-agents` | `.agents/mcp.json` | `${VAR}` | `.agents/mcp-servers/<id>/server.toml` |
| `codex` | `.agents/mcp.json` (shared with open-agents) | `${VAR}` | `.agents/mcp-servers/<id>/server.toml` |

The config entry is what the harness reads. `server.toml` is the canonical
declaration Tuff hashes, so `tuff check` and `tuff diff` treat the capability
exactly like a skill or tool.

:::caution[Codex]
The Codex adapter currently emits the same `.agents/mcp.json` entry as
Open Agents; it does not write `[mcp_servers.<id>]` into Codex's own
`~/.codex/config.toml`. This matches how MCP-native tools behave today and is
documented rather than hidden — native Codex emission is tracked separately.
:::

## Safety rules

- **Fail closed.** A malformed MCP config file is a hard error before anything
  is written; Tuff never replaces it with an empty object.
- **Never overwrite untracked entries.** If `mcpServers.<id>` already exists
  and Tuff's lockfile does not record it for that harness, the install is
  refused before a single file lands. Remove the entry by hand or choose a
  different id.
- **Unrelated keys survive.** Servers you added by hand next to Tuff's are
  left untouched on every add, update, and delete.

## Drift detection

Both artifacts are tracked. `server.toml` is hashed like any capability
tree, and the `mcpServers.<id>` entry itself carries a per-entry baseline
(recorded as `managedMcpEntry` in `tuff.lock`, the same treatment managed
hooks get). A hand-edit to either shows as `modified` in `tuff list`, fails
`tuff check`, and gates `tuff delete` behind `--force`. Neighbouring entries
you maintain by hand are never inspected. To accept the canonical entry
again, run `tuff update <id>`.

## Diagnosing a server

Everything above proves the *configuration* is right — a well-formed entry,
in the right file, matching a tracked baseline. It doesn't prove the server
actually starts. `tuff mcp doctor` closes that gap: it spawns each installed
server for real, completes the MCP `initialize` handshake, and calls
`tools/list`.

```sh frame="terminal"
tuff mcp doctor
```

```
┌────────┬───────────┬─────────────┬────────┬─────────────┐
│ ID     │ TRANSPORT │ HARNESSES   │ STATUS │ DETAIL      │
├────────┼───────────┼─────────────┼────────┼─────────────┤
│ github │ stdio     │ open-agents │ ✓ ok   │ 26 tool(s)  │
└────────┴───────────┴─────────────┴────────┴─────────────┘
```

One row per server, not per harness — the underlying process is the same
regardless of which harness's dialect wired it in, so doctor probes it once
and lists every harness it's registered for in the `HARNESSES` column.

| Status | Meaning |
|---|---|
| `ok` | Handshake and `tools/list` both succeeded |
| `missing env` | A required `[server.env]` variable isn't set in your shell — the server was never spawned |
| `spawn failed` | The `command` couldn't be launched (not on `PATH`, permission denied, …) |
| `timeout` | No valid response within `--timeout` seconds (default 10) |
| `protocol error` | The server responded, but not with a valid MCP handshake |
| `unsupported transport` | `http`-transport servers aren't probed yet — see below |

Flags: `--agent <id>` (repeatable, only check servers wired into a given
harness), `--global`, `--json`, `--timeout <seconds>`, and
`--ignore-failures` (report but exit `0`, useful outside CI).
`tuff mcp doctor` exits non-zero if any server is unhealthy, so it's safe to
wire into CI alongside `tuff check`.

## Current limits

- Catalog-installed servers cannot be included in a project pack yet.
- `doctor` only probes `stdio`-transport servers. `http` transport exists in
  the schema but isn't dialed yet — the real Streamable HTTP spec (SSE or
  JSON responses, session ids) is meaningfully more work than stdio, and no
  catalog or example entry currently uses it.
- `doctor` covers `mcp-server` capabilities only, not MCP-native tools
  (`type = "tool"` with `mcp = true`) — a natural follow-up, not yet done.
