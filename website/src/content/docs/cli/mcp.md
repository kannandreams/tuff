---
title: MCP Server Commands
description: Install external MCP servers with tuff add mcp, browse the catalog, search the registry, and check that servers start.
---

See [MCP Servers](/primitives/mcp-servers) for the concepts, the full catalog list, and the complete status table.

## Install an MCP server

Wire an external MCP server into every selected harness from one declaration.
Sources are built-in catalog ids, local directories holding a `tuff.toml`, or
git URLs naming the directory; several can be given at once:

```sh frame="terminal"
tuff add mcp github filesystem -a claude -a cursor -a open-agents
tuff add mcp ./mcp-servers/internal-search
```

What the install does:

- Each harness gets an entry in its own MCP config (`.mcp.json`, `.cursor/mcp.json`, `.agents/mcp.json`, `mcp` in `.opencode/opencode.json`, or `[mcp_servers.<id>]` in `.codex/config.toml`) plus a tracked `<prefix>/mcp-servers/<id>/server.toml`.
- Secrets are emitted as environment references, and Tuff prints which variables to export.
- The install is refused before anything is written if the config is malformed or already has an entry Tuff does not track.
- Installing from the catalog at a real terminal asks, once per required variable, whether to use a different environment variable name than the catalog's default, never the secret's value. Skip with `--yes`, or it's automatic in a non-interactive shell.

## `tuff mcp catalog`

List the built-in catalog: the curated servers `tuff add mcp <ID>` installs by name, without reaching the network.

```sh frame="terminal"
tuff mcp catalog
tuff mcp catalog --json
```

The `VARIABLES` column names the environment variables an entry expects you to export, and is empty for a server that needs none. `--json` adds the full invocation the harness would run and the tools the entry advertises, which is what the [catalog page](/mcp-catalog/) and the [VS Code extension](/guides/vscode-extension/) list. Every row is resolved through the same code path `tuff add mcp` uses, so the listing cannot offer a server the installer would refuse.

## `tuff mcp search`

Search the [MCP registry](https://registry.modelcontextprotocol.io) for servers, and see which ones Tuff can install before installing one.

```sh frame="terminal"
tuff mcp search filesystem
tuff mcp search notion --limit 5 --json
tuff mcp search notion --registry https://registry.example.internal
```

The `INSTALL` column shows the launcher Tuff would use, or `unsupported` when the entry needs something Tuff cannot express. Install a result by its full name with `tuff add mcp <NAME>`. See [MCP Servers](/primitives/mcp-servers#the-mcp-registry).

## `tuff mcp doctor`

Spawn each installed `mcp-server` capability for real, complete the MCP
`initialize` handshake, and call `tools/list`: the difference between "the
config entry exists" and "the server actually starts." One row per server,
not per harness, since the underlying process is the same regardless of
which harness's dialect wired it in.

```sh frame="terminal"
tuff mcp doctor                    # check every installed mcp-server
tuff mcp doctor -a claude          # only servers wired into claude
tuff mcp doctor --json             # machine-readable output
tuff mcp doctor --timeout 20       # wait longer before reporting a timeout
tuff mcp doctor --ignore-failures  # report failures but exit 0
```

Statuses: `ok`, `missing env` (a required variable isn't exported, so the
server is never contacted), `spawn failed`, `timeout`, `protocol error`,
and, for `http` servers, `unauthorized` and `unreachable`. Exits non-zero
if any server is unhealthy, so it composes with [`tuff check`](/cli/ci/#tuff-check) in CI.
