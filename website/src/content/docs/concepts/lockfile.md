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

`tuff.lock` is JSON, schema version 3:

```json
{
  "version": 3,
  "capabilities": [
    {
      "name": "python-uv-default",
      "type": "skill",
      "version": "1.2.0",
      "version_scheme": "declared",
      "description": "Default Python project setup with uv.",
      "target": "open-agents",
      "installed_path": ".agents/skills/python-uv-default",
      "sha256": "...",
      "ownership": "generated",
      "source": {
        "kind": "local",
        "path": "examples/skills/python-uv-default"
      }
    }
  ]
}
```

Each element of `capabilities` represents one capability installed to one adapter. Entries are deterministically ordered by name, type, target, and installed path, so diffs are line-oriented and merge conflicts stay local.

The file is written in the layout `JSON.stringify(value, null, 2)` produces: two-space indentation, one array element per line, and a single trailing newline. That is the layout npm's `package-lock.json` uses and the one `jq`, Python's `json.dumps(indent=2)`, VS Code's JSON formatter, and Prettier's `json-stringify` parser all emit, so a formatter or a pre-commit hook that rewrites every JSON file in a repository leaves `tuff.lock` byte for byte unchanged instead of producing a diff Tuff has to undo. Prettier's plain `json` parser is the one exception, as it collapses short arrays onto one line; if you route `tuff.lock` through Prettier explicitly, use `json-stringify` for it, which is how Prettier itself treats `package-lock.json`. Nothing in the repository needs to exclude the file.

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

### The source object

Every entry has one `source` object whose `kind` says where the capability came from. The other fields depend on the kind:

| `kind` | Fields | Written by |
|---|---|---|
| `local` | `path`: the source directory, relative to the project when inside it. Empty for an adopted capability whose only copy is the installed tree. | `tuff add <path>`, `tuff create` |
| `git` | `url`, `path` (subdirectory within the repository), `ref` (the commit installed). `tag` and `requested` are reserved for release-tag resolution. | `tuff add <url>` |
| `catalog` | `id` (the built-in catalog entry), `version` (that entry's version at install) | `tuff add mcp <id>` |
| `pack` | `name`, `version`, `digest` (artifact digest), `registry` (when installed with `--reference`), `path` (the member's path inside the pack) | `tuff add pack` |

A pack member's `version` is its own capability version; the pack release version lives in the source object. `tuff update` on any member moves the whole pack, because every member shares this object.

### Per-target extras

Capabilities that register into shared harness files carry the baselines Tuff uses to detect a hand edit: `managed_hooks` for hook registrations and `managed_mcp_entry` for an `mcpServers` entry. Tools, workflows, and MCP servers also cache `implementation`, `parameters`, `workflow`, or `server` from their manifest so the generated capability index can describe them after the manifest is gone.

### Migrating from an older schema

Two older schemas exist, both TOML: version 1, written by 0.1.x, and version 2, written by 0.2 through 0.7. Tuff reads either transparently: every command works on it. Read-only commands such as `list`, `check`, and `outdated` never rewrite the file, so a CI checkout stays clean. The first command that writes the lockfile writes version 3. To land the migration as its own commit:

```sh frame="terminal"
tuff lock migrate
```

It rewrites the file in the current schema and changes nothing else; on a version 3 file it is a no-op. Version 2 carries the same rows as version 3, so migrating it is a change of syntax only: the same fields with the same names, in the same order, as JSON. Migrating version 1 also maps the old columns: `source = "git"` with `repository`, `source_path`, and `resolved_ref` becomes a `git` source; `source = "catalog"` becomes a `catalog` source; a row with a `pack` table becomes a `pack` source; everything else becomes `local`. `version_scheme` is backfilled as `sha` for git rows and `declared` otherwise. Two fields that never round-tripped in version 1, `emittedFiles` and `scope`, are dropped; the tree hash and the file the row sits in already carried that information.

A lockfile from a newer Tuff is refused with a message naming the version rather than a parse error. A file whose syntax does not match its version, a TOML file claiming version 3 or a JSON file claiming version 2, is reported as corrupt with a message saying which way round it is. Tuff 0.7 and earlier cannot read a version 3 lockfile at all and report it as invalid, so once a project's lockfile has migrated, everyone who runs Tuff on that project, including the VS Code extension, needs 0.8 or newer.

Versions 1 and 2 stay readable; a release that stops reading either will say so in the changelog first.

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
