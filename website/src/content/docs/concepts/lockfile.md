---
title: Lockfile Reference
description: What Coral records in project state.
---

Coral records state in the `.coral/` directory at the root of your project. The lockfile
(`coral-lock.json`) tracks installed capabilities, their versions, where they came from,
and the hashes needed for drift detection and diffs.

## Directory structure

| File | Purpose | Commit to git? |
|---|---|---|
| `.coral/coral-lock.json` | Installed capability state (id, version, targets, source, hashes) | Yes |
| `.coral/config.json` | Registered agent harnesses and the default agent | Yes |
| `.coral/baselines/<target>/<id>/` | Pristine copies of installed files for diffing | Yes |
| `~/.coral/coral-lock.json` | Global scope: personal, across all projects | No |
| `~/.coral/cache/git/` | Cloned git repositories for skill discovery | No |

Commit the project `.coral/` files so your team can run `coral list`, `coral diff`,
and `coral update` without re-installing anything.

## Lockfile schema

```json
{
  "version": 1,
  "capabilities": {
    "python-uv-default": {
      "type": "skill",
      "installedVersion": "0.1.0",
      "sourcePath": "examples/skills/python-uv-default",
      "scope": "project",
      "targets": {
        "open-agents": {
          "baselineDir": ".coral/baselines/open-agents/python-uv-default",
          "emittedFiles": [
            {
              "path": ".agents/skills/python-uv-default/SKILL.md",
              "hash": "a1b2c3..."
            }
          ],
          "ownership": "generated"
        }
      }
    }
  }
}
```

### Per-capability fields

| Field | Description |
|---|---|
| `type` | `"skill"`, `"tool"`, `"hook"`, or `"workflow"` |
| `installedVersion` | Semantic version or git commit SHA |
| `sourcePath` | Local path to the capability directory (empty for git sources) |
| `scope` | `"project"` or `"global"` |
| `targets` | Per-harness emitted files and baseline directory |
| `source` | Git source metadata (`type`, `url`, `ref`, `skill`), absent for local installs |

### Per-target fields

| Field | Description |
|---|---|
| `baselineDir` | Path to baseline file copies for this agent |
| `emittedFiles` | List of `{ path, hash }` for each file written by the adapter |
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
