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
version = 1

[[capabilities]]
name = "python-uv-default"
type = "skill"
source = "local"
source_path = "examples/skills/python-uv-default"
resolved_ref = ""
sha256 = "..."
target = "open-agents"
installed_path = ".agents/skills/python-uv-default"
```

Each `[[capabilities]]` entry represents one capability installed to one adapter.
Entries are deterministically ordered by name, type, target, and installed path.
```

### Per-capability fields

| Field | Description |
|---|---|
| `name` | Capability identifier |
| `type` | `"skill"`, `"tool"`, `"hook"`, `"workflow"`, or `"policy"` |
| `source` | `local` or `git` |
| `source_path` | Local path, or path within the Git repository |
| `repository` | Git repository URL when `source = "git"` |
| `resolved_ref` | Commit resolved at install time for Git sources |
| `sha256` | Hash of the materialized capability directory |
| `target` | Canonical adapter ID |
| `installed_path` | Materialized directory written for this target |

### Optional pack provenance

A capability installed through `tuff add pack` also records the pack release that delivered it:

```toml
pack = { name = "crm-integration", version = "1.2.0", digest = "<sha256>" }
```

The pack version is separate from the capability's `version`. These optional fields do not change lockfile schema version 1 and are absent for capabilities installed directly from local or Git sources.

### Per-target fields

| Field | Description |
|---|---|
| `ownership` | `generated` when Tuff emitted the files, or `imported` when Tuff tracks existing files |

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
