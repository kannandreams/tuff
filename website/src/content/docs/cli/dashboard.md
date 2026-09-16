---
title: Dashboard
description: Build a report of a project's capabilities for a Tuff dashboard server.
---

:::caution[In progress]
The dashboard server is being built. Today `tuff dashboard publish` prints the report it will send, and sends nothing.
:::

## `tuff dashboard publish`

A report describes one project at one commit: its `tuff.lock`, the result of `tuff check` for the project, and where the project sits in its git repository. A dashboard server collects reports from many projects and shows them in one place.

```sh frame="terminal"
# This project
tuff dashboard publish --dry-run

# Every project under this folder, such as each app in a monorepo
tuff dashboard publish --dry-run --all
```

| Flag | Description |
|---|---|
| `--dry-run` | Print the report as JSON and send nothing. Required for now |
| `--all` | Report every folder under the current one that has a `tuff.lock`. `.git`, `node_modules`, `target`, and hidden folders are not searched |
| `--outdated` | Also report newer versions, as `tuff outdated --json` does. Needs the network |
| `--project <name>` | Name the project's repository. Required outside git or without an `origin` remote |

### How a project is named

A project is its repository and its folder in that repository. The repository is the `origin` remote with the scheme, credentials, port, and trailing `.git` removed, so `git@github.com:acme/agents.git` and `https://token@github.com/acme/agents` are both `github.com/acme/agents`. A monorepo with `apps/support-agent/tuff.lock` and `apps/billing-agent/tuff.lock` reports two projects in that repository, with the paths `apps/support-agent` and `apps/billing-agent`.

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
