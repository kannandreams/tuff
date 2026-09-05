---
title: Lockfile Reference
description: What Tuff records in project state.
---

Tuff records committed capability identity in `tuff.lock` at the root of your project.
Disposable materialized baselines live in Tuff's user cache directory and can be deleted at any time.

## Directory structure

| File | Purpose | Commit to git? |
|---|---|---|
| `tuff.lock` | Installed capability identity, source, target, and materialized hash | Yes |
| `tuff.config.json` | Optional project adapter preferences | Usually yes |
| User config/state directories | Global preferences and lockfile | No |
| User cache directory | Verified materialized baseline trees | No |

Commit `tuff.lock` so your team can verify installations. A cold or deleted cache is
refilled by refetching and verifying the recorded source.

## Lockfile schema

```toml
version = 2

[[capabilities]]
name = "python-uv-default"
type = "skill"
version = "1.2.0"
version_scheme = "declared"
description = "Default Python project setup with uv."
target = "open-agents"
installed_path = ".agents/skills/python-uv-default"
sha256 = "..."
ownership = "generated"

[capabilities.source]
kind = "local"
path = "examples/skills/python-uv-default"
```

Each `[[capabilities]]` entry represents one capability installed to one adapter.
Entries are deterministically ordered by name, type, target, and installed path, so
diffs are line-oriented and merge conflicts stay local.

### Per-capability fields

| Field | Description |
|---|---|
| `name` | Capability identifier |
| `type` | `"skill"`, `"tool"`, `"hook"`, `"workflow"`, `"mcp-server"`, or `"policy"` |
| `version` | The capability's own version |
| `version_scheme` | What `version` holds: `semver` (a release chosen by tag; `source.tag` names it), `declared` (what the manifest or `SKILL.md` frontmatter says), or `sha` (the pinned commit itself) |
| `description` | Cached from the manifest at install time |
| `sha256` | Hash of the materialized capability directory, bare lowercase hex |
| `target` | Canonical adapter ID |
| `installed_path` | Materialized directory written for this target |
| `ownership` | `generated` when Tuff emitted the files, or `imported` when Tuff tracks existing files in place |

### The source table

Every entry has one `[capabilities.source]` table whose `kind` says where the capability came from. The other fields depend on the kind:

| `kind` | Fields | Written by |
|---|---|---|
| `local` | `path`: the source directory, relative to the project when inside it. Empty for an adopted capability whose only copy is the installed tree. | `tuff add <path>`, `tuff create` |
| `git` | `url`, `path` (subdirectory within the repository), `ref` (the commit installed). `tag` and `requested` are reserved for release-tag resolution. | `tuff add <url>` |
| `catalog` | `id` (the built-in catalog entry), `version` (that entry's version at install) | `tuff add mcp <id>` |
| `pack` | `name`, `version`, `digest` (artifact digest), `registry` (when installed with `--reference`), `path` (the member's path inside the pack) | `tuff add pack` |

A pack member's `version` is its own capability version; the pack release version lives in the source table. `tuff update` on any member moves the whole pack, because every member shares this table.

### Per-target extras

Capabilities that register into shared harness files carry the baselines Tuff uses to detect a hand edit: `managed_hooks` for hook registrations and `managed_mcp_entry` for an `mcpServers` entry. Tools, workflows, and MCP servers also cache `implementation`, `parameters`, `workflow`, or `server` from their manifest so the generated capability index can describe them after the manifest is gone.

### Migrating from version 1

Tuff reads a version 1 lockfile (written by 0.1.x) transparently: every command works on it. Read-only commands such as `list`, `check`, and `outdated` never rewrite the file, so a CI checkout stays clean. The first command that writes the lockfile writes version 2. To land the migration as its own commit:

```sh frame="terminal"
tuff lock migrate
```

It rewrites the file in the current schema and changes nothing else; on a version 2 file it is a no-op. The mapping: `source = "git"` with `repository`, `source_path`, and `resolved_ref` becomes a `git` source table; `source = "catalog"` becomes a `catalog` table; a row with a `pack` table becomes a `pack` source; everything else becomes `local`. `version_scheme` is backfilled as `sha` for git rows and `declared` otherwise. Two fields that never round-tripped in version 1, `emittedFiles` and `scope`, are dropped; the tree hash and the file the row sits in already carried that information. A lockfile from a newer Tuff is refused with a message naming the version rather than a parse error.

Version 1 stays readable throughout the 0.2.x releases.

## Config schema

```json
{
  "agents": ["open-agents", "claude"],
  "defaultAgent": "open-agents"
}
```

Initialized by `tuff init`, updated by `tuff agent add <id>` or `tuff create`,
and read by `tuff agent list`. Set the default with `tuff agent set-default
<id>`; use `--global` for the global configuration. Commands with no explicit
`-a/--agent` use this value.

Agent registration is separate from capability tracking. `tuff agent remove`
only unregisters an agent; it does not change the lockfile or delete files.

Use `tuff delete <id>` to delete Tuff-generated files for the default agent.
Use `tuff untrack <id>` to remove tracking while preserving files. Pass
`-a/--agent` to select another agent.
