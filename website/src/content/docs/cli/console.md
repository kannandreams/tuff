---
title: Console
description: Run a Tuff console server, manage its publish keys, and build a report of a project's capabilities.
---

:::caution[In progress]
The server stores and returns reports today. `tuff console publish` still prints the report it will send and sends nothing, and the web pages that show the stored reports come in a later release.
:::

## `tuff console serve`

Tuff Console is the self-hosted server that collects project reports. Its overview page is called the Dashboard.

Starts the console server. It stores the reports that projects publish in one SQLite file and serves them over HTTP.

```sh frame="terminal"
# Listen on 127.0.0.1:7474
tuff console serve

# Another port and another data folder
tuff console serve --addr 127.0.0.1:9000 --data ./console-data
```

| Flag | Description |
|---|---|
| `--addr <addr>` | Address to listen on. Default `127.0.0.1:7474`. Port `0` picks a free port, and the startup line shows it |
| `--data <dir>` | Folder for `console.sqlite`. Default `$XDG_DATA_HOME/tuff/console`, which is `~/.local/share/tuff/console` when `XDG_DATA_HOME` is unset |
| `--public-read` | Allows a non-loopback address. Viewers are not authenticated by Tuff |

The server runs until it receives an interrupt or `SIGTERM`. The folder and the database are created on first start, and the schema is migrated when a newer Tuff opens an older file. A file written by a newer Tuff than the running one is refused.

### Who can connect

On a loopback address (`127.0.0.1` or `::1`), reading needs no credentials, and publishing needs none until the first key exists.

Any other address, such as `0.0.0.0:7474`, needs two things before the server starts:

- `--public-read`, because Tuff does not authenticate people who view the console. Put the server behind a reverse proxy that does, such as Caddy or nginx with basic auth or oauth2-proxy.
- At least one publish key, created with `tuff console key create`.

`POST /api/v1/reports` needs `Authorization: Bearer <key>` whenever at least one key exists, on any address, loopback included. A missing, unknown, or revoked key gets `401`. Keys are checked on every request, so a revoked key stops working at once, and revoking the last key on a loopback server opens publishing again. A server behind a reverse proxy on the same machine therefore needs one key before the proxy exposes it.

### HTTP API

Errors use the same JSON shape as the CLI's `--json` errors: `{"error": {"kind", "message", "hint"}}`.

| Request | Description |
|---|---|
| `POST /api/v1/reports` | Ingest one report. `201` when stored, `200` when the report equals the project's previous one. `422` for a `schema` the server does not read |
| `GET /api/v1/projects` | Every project with its first and last report time and its report count |
| `GET /api/v1/projects/{id}` | One project with its latest report |
| `GET /healthz` | Liveness. Returns `{"status": "ok"}` |

The response to a report holds `projectId`, `reportId`, `deduplicated`, and `projectFirstSeen`. A report is the same as the previous one when everything except `generatedAt` is equal. It then adds no row and only moves the project's last report time, so a CI job that publishes on every push does not grow the database. A report that differs is stored, including one that returns to an earlier state.

## `tuff console key`

Publish keys authorise `POST /api/v1/reports`.

```sh frame="terminal"
tuff console key create ci
tuff console key list
tuff console key revoke ci
```

`create` prints the key once. The database stores only its SHA-256, so a lost key is replaced by revoking it and creating another. A key looks like `tuffc_` followed by 64 hexadecimal characters. Names use letters, digits, `-`, `_`, and `.`, up to 64 characters, and each name is unique.

`list` shows each name with its creation time and the time it last authenticated, and `--json` prints the same as JSON. All three commands take `--data <dir>` and act on the database in that folder, whether or not a server is running from it.

## `tuff console publish`

A report describes one project at one commit: its `tuff.lock`, the result of `tuff check` for the project, and where the project sits in its git repository. A console server collects reports from many projects and shows them in one place.

```sh frame="terminal"
# This project
tuff console publish --dry-run

# Every project under this folder, such as each app in a monorepo
tuff console publish --dry-run --all
```

| Flag | Description |
|---|---|
| `--dry-run` | Print the report as JSON and send nothing. Required for now |
| `--all` | Report every folder under the current one that has a `tuff.lock`. `.git`, `node_modules`, `target`, and hidden folders are not searched |
| `--outdated` | Also report newer versions, as `tuff outdated --json` does. Needs the network |
| `--project <name>` | Name the project's repository. Required outside git or without an `origin` remote |

### How a project is named

A project is its repository and its folder in that repository. The repository is the `origin` remote with the scheme, credentials, port, and trailing `.git` removed, so `git@github.com:acme/agents.git` and `https://key@github.com/acme/agents` are both `github.com/acme/agents`. A monorepo with `apps/support-agent/tuff.lock` and `apps/billing-agent/tuff.lock` reports two projects in that repository, with the paths `apps/support-agent` and `apps/billing-agent`.

### What a report contains

```json
{
  "schema": 1,
  "tuffVersion": "0.12.0",
  "generatedAt": "2026-09-16T18:00:00Z",
  "project": {
    "repository": "github.com/acme/agents",
    "path": "apps/billing-agent",
    "name": "billing-agent",
    "commit": "3f2c9e1…",
    "branch": "main",
    "dirty": false
  },
  "lockfile": { "version": 3, "capabilities": [] },
  "check": { "valid": true, "results": [] },
  "outdated": null
}
```

- `lockfile` is the project's `tuff.lock` in the current schema. It holds capability ids, versions, source URLs, installed paths, hashes, compiled policy rules, and MCP server entries. Manifests keep secrets as references, so the report names environment variables and never holds their values.
- `check` is `tuff check --json` for the project alone. The global scope is left out.
- `dirty` is true when the project folder has uncommitted changes.

Run with `--dry-run` to review exactly what a report contains before sending it anywhere.
