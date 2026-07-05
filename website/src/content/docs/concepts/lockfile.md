---
title: Lockfile Reference
description: What Coral records in project state.
---

Coral records state in the `.coral/` directory at the root of your project. The lockfile
(`coral-lock.json`) tracks installed primitives, their versions, where they came from,
and the hashes needed for drift detection and diffs.

## Directory structure

| File | Purpose | Commit to git? |
|---|---|---|
| `.coral/coral-lock.json` | Installed primitive state (id, version, targets, source, hashes) | Yes |
| `.coral/config.json` | Registered harness targets (`coral target add`) | Yes |
| `.coral/baselines/<target>/<id>/` | Pristine copies of installed files for diffing | Yes |
| `~/.coral/coral-lock.json` | Global scope — personal, across all projects | No |
| `~/.coral/cache/git/` | Cloned git repositories for skill discovery | No |

Commit the project `.coral/` files so your team can run `coral list`, `coral diff`,
and `coral update` without re-installing anything.

## Lockfile schema

```json
{
  "version": 2,
  "primitives": {
    "python-uv-default": {
      "primitive": "skill",
      "installedVersion": "0.1.0",
      "sourcePath": "examples/fixtures/python-uv-default",
      "scope": "project",
      "targets": {
        "open-agents": {
          "baselineDir": ".coral/baselines/open-agents/python-uv-default",
          "emittedFiles": [
            {
              "path": ".agents/skills/python-uv-default/SKILL.md",
              "hash": "a1b2c3..."
            }
          ]
        }
      }
    }
  }
}
```

### Per-primitive fields

| Field | Description |
|---|---|
| `primitive` | Kind — `"skill"`, `"tool"`, or `"hook"` |
| `installedVersion` | Semantic version or git commit SHA |
| `sourcePath` | Local path to the capability directory (empty for git sources) |
| `scope` | `"project"` or `"global"` |
| `targets` | Per-harness emitted files and baseline directory |
| `source` | Git source metadata (`type`, `url`, `ref`, `skill`) — absent for local installs |

### Per-target fields

| Field | Description |
|---|---|
| `baselineDir` | Path to baseline file copies for this target |
| `emittedFiles` | List of `{ path, hash }` for each file written by the adapter |

## Config schema

```json
{
  "targets": ["open-agents", "claude"]
}
```

Written by `coral target add <id>` and read by `coral target list`.
