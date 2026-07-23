---
title: Lockfile Reference
description: What Coral records in project state.
---

Coral records committed capability identity in `coral.lock` at the root of your project.
Disposable materialized baselines live in `~/.coral/cache/` and can be deleted at any time.

## Directory structure

| File | Purpose | Commit to git? |
|---|---|---|
| `coral.lock` | Installed capability identity, source, target, and materialized hash | Yes |
| `.coral/` | Local Coral configuration and scratch state | No |
| `~/.coral/coral.lock` | Global scope: personal, across all projects | No |
| `~/.coral/cache/sha256/` | Verified materialized baseline trees | No |

Commit `coral.lock` so your team can verify installations. A cold or deleted cache is
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

### Per-target fields

| Field | Description |
|---|---|
| `ownership` | `generated` when Coral emitted the files, or `imported` when Coral tracks existing files |

## Config schema

```json
{
  "agents": ["open-agents", "claude"],
  "defaultAgent": "open-agents"
}
```

Initialized by `coral init`, updated by `coral agent add <id>` or `coral create`,
and read by `coral agent list`. Set the default with `coral agent set-default
<id>`; use `--global` for the global configuration. Commands with no explicit
`-a/--agent` use this value.

Agent registration is separate from capability tracking. `coral agent remove`
only unregisters an agent; it does not change the lockfile or delete files.

Use `coral delete <id>` to delete Coral-generated files for the default agent.
Use `coral untrack <id>` to remove tracking while preserving files. Pass
`-a/--agent` to select another agent.
