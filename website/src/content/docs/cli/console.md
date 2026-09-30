---
title: Console
description: Run a Tuff console server, publish project reports to it with GitHub Actions or an API key, and manage its publish credentials.
---

:::caution[In progress]
The server stores the reports that projects publish and records what changed between them. The web pages that show the stored reports, including the Dashboard, come in a later release. The console is excluded from the [1.0 stability promise](/concepts/stability).
:::

## `tuff console serve`

Tuff Console is the self-hosted server that collects project reports. Its overview page is called the Dashboard.

Starts the console server. It stores the reports that projects publish in one SQLite file and serves them over HTTP.

```sh frame="terminal"
# Listen on 127.0.0.1:7474
tuff console serve

# Another port and another data folder
tuff console serve --addr 127.0.0.1:9000 --data ./console-data

# Accept publishing from GitHub Actions jobs of the acme organisation
tuff console serve --trust github:acme --public-url https://tuff.internal.acme.dev
```

| Flag | Description |
|---|---|
| `--addr <addr>` | Address to listen on. Default `127.0.0.1:7474`. Port `0` picks a free port, and the startup line shows it |
| `--data <dir>` | Folder for `console.sqlite`. Default `$XDG_DATA_HOME/tuff/console`, which is `~/.local/share/tuff/console` when `XDG_DATA_HOME` is unset |
| `--public-read` | Allows a non-loopback address. Viewers are not authenticated by Tuff |
| `--trust <provider:owner>` | Accepts publishing from GitHub Actions jobs of that owner, as `github:<owner>`. Repeat it for several owners |
| `--public-url <url>` | The address publishers use to reach the console, which OIDC tokens name as their audience. Default `http://<addr>` |

The server runs until it receives an interrupt or `SIGTERM`. The folder and the database are created on first start, and the schema is migrated when a newer Tuff opens an older file. A file written by a newer Tuff than the running one is refused.

### Who can connect

On a loopback address (`127.0.0.1` or `::1`), reading needs no credentials, and publishing needs none until a key exists or a trust is configured.

Any other address, such as `0.0.0.0:7474`, needs two things before the server starts:

- `--public-read`, because Tuff does not authenticate people who view the console. Put the server behind a reverse proxy that does, such as Caddy or nginx with basic auth or oauth2-proxy.
- At least one publish credential: a key created with `tuff console key create`, or a `--trust`.

Once any key exists or any trust is configured, `POST /api/v1/reports` needs `Authorization: Bearer <credential>` on every address, loopback included. A server behind a reverse proxy on the same machine therefore needs a key or a trust before the proxy exposes it. Keys are checked on every request, so a revoked key stops working at once, and revoking the last key on a loopback server with no trust opens publishing again.

### Trusting GitHub Actions

`--trust github:<owner>` lets jobs of that owner's repositories publish without a stored secret. The job asks the runner for an OIDC token whose audience is the console's URL, and `tuff console publish` sends it as the bearer. The console accepts a token when all of these hold:

- its signature verifies against the keys GitHub publishes at `https://token.actions.githubusercontent.com/.well-known/jwks`, which the console caches and fetches again when a token names a key it does not have
- `iss` is `https://token.actions.githubusercontent.com`
- `aud` is the console's public URL, as set with `--public-url`
- `exp` and `nbf` are valid, with 60 seconds of allowed clock difference
- `repository_owner` is one of the trusted owners, compared without regard to case

The report must also name the token's repository. A job in `acme/web` can publish a report whose `project.repository` is `github.com/acme/web` and no other, so one repository cannot write into another's history. Owner names and repository names are compared without regard to case.

The provider prefix is stored with each trust so other providers can be added later. Only `github` is accepted today, and GitHub Enterprise Server and GitLab are not supported yet.

### HTTP API

Errors use the same JSON shape as the CLI's `--json` errors: `{"error": {"kind", "message", "hint"}}`.

| Request | Description |
|---|---|
| `POST /api/v1/reports` | Ingest one report |
| `GET /api/v1/projects` | Every project with its first and last report time and its report count |
| `GET /api/v1/projects/{id}` | One project with its latest report |
| `GET /healthz` | Liveness. Returns `{"status": "ok"}` |

Status codes of `POST /api/v1/reports`:

| Status | Meaning |
|---|---|
| `201` | The report was stored |
| `200` | The report equals the project's previous one, so nothing was stored |
| `400` | The body is not JSON |
| `401` | No credential, an unknown or revoked key, or a token that is malformed, expired, for another audience, or not signed by the issuer |
| `403` | The credential is valid but may not publish this report: the token's owner is not trusted, or the credential is bound to another repository |
| `422` | The `schema` is one the server does not read, or the report is incomplete |
| `503` | The console could not fetch the token issuer's signing keys |

The response to a stored or unchanged report holds `projectId`, `reportId`, `deduplicated`, and `projectFirstSeen`. A report is the same as the previous one when everything except `generatedAt` is equal. It then adds no row and only moves the project's last report time, so a CI job that publishes on every push does not grow the database. A report that differs is stored, including one that returns to an earlier state.

### Audit events

Each stored report is compared with the project's previous report, and the console records the differences as events. Each event names the project, the report, and the commit, so the commit that introduced a capability, changed its version, or caused drift is on record. A report that equals the previous one records nothing.

| Event | Recorded when |
|---|---|
| `project_first_seen` | The project's first report arrives |
| `capability_added`, `capability_removed` | A capability id appears in or disappears from the project |
| `version_changed` | A capability's version or source changes. One event names every target the change applies to |
| `target_added`, `target_removed` | A capability gains or loses a harness target |
| `drift_detected`, `drift_cleared` | A capability's `tuff check` status leaves or returns to `ok` |
| `policy_gap_added`, `policy_gap_closed` | A policy rule recorded as not enforced appears or goes |

The first report of a project also records a `capability_added` event for each capability it carries and a `policy_gap_added` event for each recorded gap. The console keeps the latest report of each project as one row per capability and target, for queries across projects.

## `tuff console key`

API keys authorise `POST /api/v1/reports` from CI systems other than GitHub Actions.

```sh frame="terminal"
tuff console key create ci
tuff console key create web-ci --repository github.com/acme/web
tuff console key list
tuff console key revoke ci
```

`create` prints the key once. The database stores only its SHA-256, so a lost key is replaced by revoking it and creating another. A key looks like `tuffc_` followed by 64 hexadecimal characters. Names use letters, digits, `-`, `_`, and `.`, up to 64 characters, and each name is unique.

A key created with `--repository` can publish reports for that repository only, the way a GitHub token can. The repository is written as the reports name it, such as `github.com/acme/web`, and a remote URL such as `git@github.com:acme/web.git` is normalised to the same form. A report for another repository gets `403`.

`list` shows each name with its repository, its creation time, and the time it last authenticated, and `--json` prints the same as JSON. All three commands take `--data <dir>` and act on the database in that folder, whether or not a server is running from it.

## `tuff console publish`

A report describes one project at one commit: its `tuff.lock`, the result of `tuff check` for the project, and where the project sits in its git repository. `publish` builds the report and sends it to a console server.

```sh frame="terminal"
# This project
tuff console publish

# Every project under this folder, such as each app in a monorepo
tuff console publish --all --server https://tuff.internal.acme.dev

# Review what would be sent, and send nothing
tuff console publish --dry-run
```

| Flag | Description |
|---|---|
| `--server <url>` | The console's address. Default `http://127.0.0.1:7474`. Also read from `TUFF_CONSOLE_URL` |
| `--key <key>` | An API key. Also read from `TUFF_CONSOLE_KEY` |
| `--all` | Report every folder under the current one that has a `tuff.lock`. `.git`, `node_modules`, `target`, and hidden folders are not searched |
| `--outdated` | Also report newer versions, as `tuff outdated --json` does. Needs the network |
| `--project <name>` | Name the project's repository. Required outside git or without an `origin` remote |
| `--dry-run` | Print the report as JSON and send nothing |

The command prints one line per project and exits with a non-zero status when the console refused any of them:

```text
Publishing 2 projects to https://tuff.internal.acme.dev with the GitHub Actions OIDC token.
  stored     github.com/acme/agents apps/billing-agent (report 14)
  unchanged  github.com/acme/agents apps/support-agent
2 stored, 0 unchanged.
```

A refused project shows the status and the console's reason, such as `refused    github.com/acme/agents apps/billing-agent: HTTP 403, this credential may publish only for github.com/acme/web`. A console that cannot be reached stops the run with an error that names the address.

### Publishing from GitHub Actions

With `--trust github:<owner>` on the console, a workflow publishes with no secret. The job needs `permissions: id-token: write`, which lets the runner issue it an OIDC token:

```yaml
permissions:
  id-token: write
  contents: read
steps:
  - uses: actions/checkout@v4
  - run: tuff check
  - run: tuff console publish --all
    env:
      TUFF_CONSOLE_URL: https://tuff.internal.acme.dev
```

When no key is given and the runner has set `ACTIONS_ID_TOKEN_REQUEST_URL` and `ACTIONS_ID_TOKEN_REQUEST_TOKEN`, `publish` requests a token with the console's URL as its audience and sends it as the bearer. `TUFF_CONSOLE_URL` must be the same address the console was given with `--public-url`, apart from a trailing slash. A key given with `--key` or `TUFF_CONSOLE_KEY` is used instead of the runner token.

### Publishing from other CI systems

Create a key on the console host and store it in the CI system's secret store:

```sh frame="terminal"
tuff console key create billing-ci --repository github.com/acme/agents
```

```yaml
steps:
  - run: tuff console publish --all
    env:
      TUFF_CONSOLE_URL: https://tuff.internal.acme.dev
      TUFF_CONSOLE_KEY: ${{ secrets.TUFF_CONSOLE_KEY }}
```

`publish` warns when it would send a key or token over plain `http://` to a host other than loopback. Use an `https://` address for any console that is not on the same machine.

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
