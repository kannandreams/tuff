---
title: MCP Servers
description: Declare an external MCP server once and wire it into every harness's native MCP config.
---

An `mcp-server` capability is an **external** MCP server, one whose code you
do not ship, only point at: the GitHub server, a filesystem server, something a
teammate published. Tuff turns one declaration into the right entry in each
harness's MCP config, then tracks it like any other capability: listed,
checked for drift, updatable, deletable.

:::note[MCP server vs. MCP-native tool]
Two different things carry "MCP" in this documentation:

- **MCP server** (`type = "mcp-server"`, this page): an external server.
  Tuff writes the config entry that launches or connects to it.
- **MCP-native tool** (`type = "tool"` with `implementation.mcp = true`, see
  [Tools](/primitives/tools)): a server whose code Tuff copies into the
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
command = "docker"           # required for stdio
args = ["run", "-i", "--rm", "-e", "GITHUB_PERSONAL_ACCESS_TOKEN", "ghcr.io/github/github-mcp-server"]
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
| `server.headers` | HTTP request headers, as **references**; `http` transport only | No |
| `server.metadata.tools_summary` | One-line list of what the server exposes; surfaces in the generated capability index | No |

### Secrets are references, never values

Every `[server.env]` and `[server.headers]` value must be
`{ from_env = "NAME" }`. There is no field a literal secret can legally occupy,
so the manifest can be committed and shared. A bare string is rejected at
install time with an error that names the `from_env` form. After install, Tuff
prints which variables you still need to export.

### Remote servers and auth headers

A remote server is a `url` plus, usually, one static header carrying a token.
Declare it the same way you declare environment: by naming the variable that
holds the value, never the value.

```toml
id = "notion"
version = "1.0.1"
type = "mcp-server"
description = "Official Notion MCP server."

[server]
transport = "http"
url = "https://mcp.notion.com/mcp"

[server.headers]
Authorization = { from_env = "NOTION_TOKEN", format = "Bearer {}" }
```

`format` wraps the value, with `{}` standing for it, which is what
`Authorization: Bearer <token>` needs. It defaults to the bare value, so an
`X-Api-Key = { from_env = "API_KEY" }` header needs nothing extra. A `format`
with no `{}` would discard the secret and one with two would repeat it, so both
are rejected at parse time.

Each harness gets its own dialect, and Tuff writes the reference, never the
token, so the secret stays in your environment:

| Harness | Entry |
|---|---|
| Claude Code, Codex, Open Agents | `{"type": "http", "url": …, "headers": {"Authorization": "Bearer ${NOTION_TOKEN}"}}` |
| Cursor | `{"url": …, "headers": {"Authorization": "Bearer ${env:NOTION_TOKEN}"}}` (no `type` for remote servers) |

`tuff check` hashes the whole entry, so a header edited by hand in any of those
files shows as `modified` like any other drift.

## Installing

From the built-in catalog, a local directory, or a git URL, several at once:

```sh frame="terminal"
tuff add mcp github filesystem -a claude -a cursor -a open-agents
tuff add mcp ./mcp-servers/internal-search
tuff add mcp https://github.com/acme/capabilities//mcp-servers/internal-notion-mirror
```

Git sources must name the subdirectory holding `tuff.toml`.

### The catalog

Every entry is verified against a primary source, the vendor's own README
or the `modelcontextprotocol/servers` repo, not a search snippet or memory.
A wrong package name in something billed as "curated" wastes your time, so
entries below medium confidence, or with an invocation inferred rather than
documented, are left out rather than guessed at.

The catalog is a list of launch declarations, not server code. It lives in
the repository at `crates/tuff-core/assets/mcp-catalog.toml` and is compiled
into the `tuff` binary, so `tuff add mcp <id>` resolves an entry without
network access. Each entry records what a manifest would: the `command` and
`args` that start the server, and the names of the environment variables it
needs. The server itself is fetched when the harness launches it, by `npx`,
`uvx`, or `docker` from the vendor's public package or image, exactly as the
vendor's own README documents. Tuff ships none of that code. Because the
catalog is part of the binary, `tuff outdated` compares an installed entry
against that entry's version in the catalog your `tuff` carries, and a newer
catalog arrives with a newer Tuff release.

The full list, with what each entry runs, the variables it needs, and the tools it answers with, is on the [MCP Catalog](/mcp-catalog/) page. That page is generated from the catalog file itself on every build, so it never drifts from what your `tuff` can install.

Two of the entries, `linear` and `context7`, are remote servers: a `url` plus an `Authorization` header built from the named variable, exactly as a manifest declares it (see [Remote servers and auth headers](#remote-servers-and-auth-headers)). Linear also offers an interactive OAuth flow; the catalog entry uses the API key path because a config file can carry a variable reference and cannot carry a login. Context7 calls its key recommended rather than required, but the catalog has no notion of an optional header, so the entry asks for one; the keyless stdio form is a registry install away as `io.github.upstash/context7`.

### The MCP registry

The catalog is a small set chosen for correctness. The official [MCP registry](https://registry.modelcontextprotocol.io) holds thousands, published by the server authors themselves, and `tuff add mcp` falls through to it for any name it does not recognise.

```sh frame="terminal"
tuff mcp search notion
tuff add mcp com.notion/mcp -a claude
```

```text
│ NAME                        │ VERSION │ INSTALL     │ DESCRIPTION                        │
│ ai.smithery/smithery-notion │ 1.0.0   │ http        │ A Notion workspace is a collabor…  │
│ com.mcparmory/notion        │ 1.0.2   │ uvx         │ Create, update, and manage pages…  │
│ com.notion/mcp              │ 1.0.1   │ http        │ Official Notion MCP server         │
```

The `INSTALL` column is the launcher Tuff would use, or `unsupported` with the reason when it cannot express the entry. Knowing that before you install beats finding out after.

A registry entry describes a package and its arguments rather than a command line, so Tuff assembles one: the launcher comes from the entry's `runtimeHint` or its package type (`npm` runs under `npx`, `pypi` under `uvx`, `oci` under `docker`, `nuget` under `dnx`), and the package is pinned to the version the registry lists. Environment variables contribute their names only, as `{ from_env = "NAME" }` references, exactly as a catalog entry does. A registry entry can carry a default *value* for a variable; Tuff never copies one into a manifest, because a manifest is committed.

Names are matched exactly, so a search hit never installs by surprise, and only the current release of each server is offered. The capability id is the last part of the name, unless that only names the protocol: `com.notion/mcp` installs as `notion-mcp` rather than the useless `mcp`.

`tuff outdated` re-resolves a registry install against its registry, so unlike a built-in entry it can go out of date without upgrading Tuff. `tuff update` moves it forward.

#### Remote entries and their headers

A remote entry's **required** headers become `[server.headers]` references. How depends on what the publisher wrote down:

| The entry says | Tuff writes |
|---|---|
| `Authorization` with a value of `Bearer {vendor_api_key}` | `Authorization = { from_env = "VENDOR_API_KEY", format = "Bearer {}" }` |
| `Authorization`, and nothing about its value | `Authorization = { from_env = "<ID>_AUTHORIZATION" }` |
| `Accept` with a value of `application/json` | Refused: a manifest has no field a literal value can occupy |

The second row is the common case by a wide margin, and the variable holds the *entire* header value, prefix included. Tuff does not add a `Bearer ` that nobody wrote down: guessing would be right often and wrong silently, and a wrong guess produces a config that looks correct and fails inside the agent.

A placeholder name that says nothing about whose key it is, such as `{api_key}`, is qualified with the capability id, so two servers do not quietly share one variable.

**Optional headers are left out**, and named at install time so you can add them by hand. Requiring a variable the server does not require would report a working server as `missing env`.

Entries that publish both the `streamable-http` and the superseded `sse` transport install the former. An entry offering only `sse` is refused: it is a different handshake, and installing one as the other would write a config no harness could use.

**Refused rather than approximated.** An entry Tuff cannot express exactly is refused with the reason, because a wrong launch command wastes more time than a clear no. That covers entries needing a value substituted into a `{placeholder}`, which Tuff has nowhere to ask for; literal header values; `sse`-only remotes; and package kinds with no launcher, such as `mcpb` bundles. OAuth-only servers stay refused too: the harnesses implement that browser flow themselves, and a capability manager's job is writing the configuration the harness reads.

Both `tuff mcp search` and `tuff add mcp` take `--registry`, so a team running its own registry can point at it instead.

### Choosing your own variable name

At a real terminal, installing from the catalog asks, once per required
variable, whether to use a different environment variable name than the
catalog's default:

```
github: reads GITHUB_PERSONAL_ACCESS_TOKEN from your environment. Press enter to keep it, or type a different variable name: GH_TOKEN
```

It never asks for the secret's *value*, only which variable holds it, so a
catalog default that doesn't match what you already have exported doesn't
force a rename of your own environment. Press enter to keep the default.
Skipped automatically in a non-interactive shell or CI, or explicitly with
`--yes`.

Every lifecycle verb works unchanged:

```sh frame="terminal"
tuff list --type mcp-server
tuff check
tuff outdated          # catalog installs compare against that entry's own version
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
documented rather than hidden. Native Codex emission is tracked separately.
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

Everything above proves the *configuration* is right: a well-formed entry,
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

One row per server, not per harness. The underlying process is the same
regardless of which harness's dialect wired it in, so doctor probes it once
and lists every harness it's registered for in the `HARNESSES` column.

| Status | Meaning | Transport |
|---|---|---|
| `ok` | Handshake and `tools/list` both succeeded | both |
| `missing env` | A variable `[server.env]` or `[server.headers]` references isn't set in your shell. Reported before anything is spawned or sent | both |
| `timeout` | No valid response within `--timeout` seconds (default 10) | both |
| `protocol error` | It responded, but not with a valid MCP handshake | both |
| `spawn failed` | The `command` couldn't be launched (not on `PATH`, permission denied, …) | stdio |
| `unauthorized` | The server answered `401` or `403`: the token is wrong, expired, or lacks scope | http |
| `unreachable` | DNS, TLS, or connection failure — nothing answered | http |

`unauthorized` is its own status rather than a kind of `protocol error`
because it's the most likely failure for a remote server and the fix is
entirely different: check the token, not the config.

### How an HTTP server is probed

The same three steps as stdio — `initialize`, `notifications/initialized`,
`tools/list` — over the Streamable HTTP transport. Doctor accepts either
response shape a server may choose (a plain JSON body or an SSE stream),
carries the `Mcp-Session-Id` the server issues on initialize, and echoes the
protocol version the server negotiated.

Headers are sent with their real values, read from your environment at the
moment of the request, so doctor checks exactly what the harness will send.
There is no `--header` flag: a one-off token on the command line would put a
credential in your shell history and would check something other than what
the harness uses. Export the variable instead.

Flags: `--agent <id>` (repeatable, only check servers wired into a given
harness), `--global`, `--json`, `--timeout <seconds>`, and
`--ignore-failures` (report but exit `0`, useful outside CI).
`tuff mcp doctor` exits non-zero if any server is unhealthy, so it's safe to
wire into CI alongside `tuff check`.

## Current limits

- Catalog- and registry-installed servers cannot be included in a project pack yet.
- The catalog's own `linear` and `context7` entries still need their headers
  filled in; the registry path already handles equivalents.
- OAuth is out of scope. Several remote servers support OAuth 2.1 with
  PKCE, and the harnesses implement that browser flow themselves. A server
  that requires OAuth and offers no static header stays refused, with that
  reason.
- `doctor` covers `mcp-server` capabilities only, not MCP-native tools
  (`type = "tool"` with `mcp = true`). A natural follow-up, not yet done.
