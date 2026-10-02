---
title: Console
description: Run a Tuff console server with its web UI, publish project reports to it with GitHub Actions or an API key, and manage its publish credentials.
---

## `tuff console serve`

Tuff Console is the self-hosted server that collects project reports. Its overview page is called the Dashboard. The console is excluded from the [1.0 stability promise](/concepts/stability), so its commands, API, and storage can change in any release. [Self-Hosting the Console](/guides/self-hosting-console/) covers running it on a server, in a container, and behind a reverse proxy. Each release also publishes the console as the container image `ghcr.io/kannandreams/tuff-console` for `linux/amd64` and `linux/arm64`, and [Run in a container](/guides/self-hosting-console/#run-in-a-container) shows how to start it.

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
| `--demo` | Serves generated sample projects from a temporary in-memory database, for trying the UI. The UI shows a Sample data chip. Cannot be combined with `--data` |
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

### Web UI

Open the server's address in a browser. The pages are embedded in the `tuff` binary and call the JSON API below, and they load nothing from the internet. The views are linkable, because the route is in the URL hash.

| View | Address | Shows |
|---|---|---|
| Dashboard | `#/` | Counts of projects, capabilities, and harnesses, the drifted, outdated, and policy-gap totals, the projects that need attention, and the latest events |
| Projects | `#/projects`, `#/projects/{id}` | Every project with its harnesses, capability count, last report, and status. A project page lists its capabilities per harness, recorded policy gaps, timeline, and report history |
| Capabilities | `#/capabilities`, `#/capabilities/{type}/{id}` | Every capability with the versions in use, marked mixed when projects differ, filtered by type. A capability page lists where it is used |
| Harnesses | `#/harnesses` | A matrix of projects by harness |
| Policies | `#/policies` | Which projects carry which policy, every rule not enforced, and the projects without a policy |
| Audit | `#/audit` | The event log, filtered by project, event kind, capability, and date |
| Settings | `#/settings` | The trusts and the API key names with their creation and last use, read only |

`tuff console serve --demo` fills the views with generated sample projects, including a monorepo, drift, outdated versions, policy gaps, and a history of reports. The data lives in memory and is gone when the server stops, and the UI shows a Sample data chip.

The top bar shows the server's address. On a loopback address it reads "local, only this machine can connect". On any other address it says whether publishing requires authentication. A console with no reports explains `tuff console publish` and links this page.

### HTTP API

Errors use the same JSON shape as the CLI's `--json` errors: `{"error": {"kind", "message", "hint"}}`.

| Request | Description |
|---|---|
| `POST /api/v1/reports` | Ingest one report |
| `GET /api/v1/projects` | Every project with its summary, and the totals the Dashboard shows |
| `GET /api/v1/projects/{id}` | One project with its capabilities per target, recorded policy gaps, and latest report |
| `GET /api/v1/projects/{id}/reports` | The stored reports of a project, newest first, without their bodies |
| `GET /api/v1/capabilities` | Every capability across projects. `?type=skill` keeps one type |
| `GET /api/v1/capabilities/{type}/{id}` | Where one capability is used. The id may contain `/` |
| `GET /api/v1/harnesses` | The project by harness matrix |
| `GET /api/v1/policies` | Policies, recorded gaps, and projects without a policy |
| `GET /api/v1/events` | The audit log, newest first. Filters: `project` (an id), `capability`, `kind`, `since` (a date or time), `before` (an event id, to read the next page), and `limit` (default 200, at most 1000) |
| `GET /api/v1/settings` | The server's address and access mode, the trusts, and the key names. Key secrets are never returned |
| `GET /healthz` | Liveness. Returns `{"status": "ok", "version": "<tuff version>"}`. `GET /api/v1/healthz` answers the same |

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

The shapes the UI reads, with field names as returned:

- `GET /projects` returns `{"projects": [...], "summary": {...}}`. Each project has `id`, `repository`, `path`, `name`, `firstReportAt`, `lastReportAt`, `reportCount`, `commit`, `branch`, `dirty`, `harnesses`, `capabilityCount`, `driftCount`, `outdatedCount`, `policyGapCount`, and `status`, which is `drift`, `gap`, `outdated`, or `ok`. `summary` has `projects`, `repositories`, `capabilities`, `harnesses`, `driftCount`, `outdatedCount`, `policyGapCount`, and `lastReportAt`.
- `GET /projects/{id}` returns `project` (the same fields), `capabilities` (one entry per capability and harness with `type`, `id`, `target`, `version`, `source`, `status`, `outdated`, and `latest`), `policyGaps`, and `latestReport`. `outdated` and `latest` come from the report's `--outdated` data.
- `GET /projects/{id}/reports` returns `{"projectId", "reports": [{"id", "receivedAt", "generatedAt", "commit", "branch", "tuffVersion", "digest"}]}`.
- `GET /capabilities` returns `{"capabilities": [...], "types": [...]}`. Each capability has `type`, `id`, `versions` (`{"version", "projects"}`), `mixed`, `projectCount`, `targets`, and `projects`. `types` lists every type in use, whatever the filter.
- `GET /capabilities/{type}/{id}` returns `type`, `id`, `versions`, `mixed`, `projectCount`, and `usage`, one entry per project and harness with `projectId`, `name`, `repository`, `path`, `target`, `version`, `source`, `status`, `outdated`, and `latest`. A capability nobody uses is `404`.
- `GET /harnesses` returns `{"harnesses": [...], "projects": [{"id", "name", "repository", "path", "counts": {"claude": 4}}], "totals": {...}}`, where a count is the capabilities installed for that harness.
- `GET /policies` returns `policies` (each with `id`, `projectCount`, `versions`, `mixed`, and `usage`), `projectsWithoutPolicy`, and `gaps` (`projectId`, `name`, `repository`, `path`, `policy`, `target`, `rule`, `description`, `reason`).
- `GET /events` returns `{"events": [...], "kinds": [...], "nextBefore": ...}`, where `nextBefore` is the id to pass as `before` for the next page, or `null` on the last one. Each event has `id`, `projectId`, `reportId`, `kind`, `capabilityType`, `capabilityId`, `target`, `detail`, `commit`, `occurredAt`, `projectName`, `repository`, and `path`. A malformed filter is `400`.
- `GET /settings` returns `server` (`version`, `address`, `loopback`, `publicRead`, `publishRequiresAuth`, `demo`, `audience`), `trusts` (`provider`, `owner`), and `keys` (`name`, `repository`, `createdAt`, `lastUsedAt`).

The response to a stored or unchanged report holds `projectId`, `reportId`, `deduplicated`, and `projectFirstSeen`. A report is the same as the previous one when everything except `generatedAt` and the project's commit, branch, and `dirty` flag is equal. It then adds no row: the previous report takes the new commit, branch, and time, so the project shows the commit it was last seen at, and no events are recorded. A CI job that publishes on every push therefore adds a row only when the project's capabilities, their status, or its policy gaps change. A report that differs is stored, including one that returns to an earlier state.

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

With `--trust github:<owner>` on the console, a workflow publishes with no secret. The job needs `permissions: id-token: write`, which lets the runner issue it an OIDC token. This workflow publishes after `tuff check` passes on the default branch:

```yaml title=".github/workflows/tuff-console.yml"
name: Tuff Console
on:
  push:
    branches: [main]

permissions:
  id-token: write
  contents: read

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Build and install tuff
        run: cargo install tuffcli

      - name: Validate capabilities
        run: tuff check

      - name: Publish to the console
        run: tuff console publish --all
        env:
          TUFF_CONSOLE_URL: https://tuff.internal.acme.dev
```

The install steps are the ones on [Validate in CI](/cli/ci/#ci-with-github-actions). Any other way of installing `tuff` works, such as the curl installer or `pip install tuffcli`. The workflow needs a `tuff` release that includes the console.

When no key is given and the runner has set `ACTIONS_ID_TOKEN_REQUEST_URL` and `ACTIONS_ID_TOKEN_REQUEST_TOKEN`, `publish` requests a token with the console's URL as its audience and sends it as the bearer. `TUFF_CONSOLE_URL` must be the same address the console was given with `--public-url`, apart from a trailing slash. A key given with `--key` or `TUFF_CONSOLE_KEY` is used instead of the runner token. The job publishes as its own repository only, so a repository needs its own workflow.

`--all` reports every folder under the checkout that has a `tuff.lock`, which is the form for a monorepo. Without it, `publish` reports the project in the current folder. Put `working-directory` on the step, or run the command from that folder, to publish one app of a monorepo.

### Publishing from other CI systems

Create a key on the console host and store it in the CI system's secret store as `TUFF_CONSOLE_KEY`:

```sh frame="terminal"
tuff console key create billing-ci --repository github.com/acme/agents
```

`publish` reads `TUFF_CONSOLE_URL` and `TUFF_CONSOLE_KEY` from the environment. A GitLab CI job that publishes the default branch:

```yaml title=".gitlab-ci.yml"
tuff-console:
  image: rust:1
  rules:
    - if: $CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH
  variables:
    TUFF_CONSOLE_URL: https://tuff.internal.acme.dev
  script:
    - cargo install tuffcli
    - tuff check
    - tuff console publish --all
```

Add `TUFF_CONSOLE_KEY` under Settings, CI/CD, Variables, masked. Jenkins, CircleCI, and other systems run the same `tuff check` and `tuff console publish --all` commands with the two variables set from their secret store. A project outside GitHub has an `origin` remote that names its own host, so its reports carry that host in `project.repository`. Pass `--project <name>` when the checkout has no `origin` remote.

The key's `--repository` value must equal the repository the report names, such as `gitlab.com/acme/agents` for a GitLab project, or the console answers `403`. Leave it out for a key that publishes for several repositories.

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
