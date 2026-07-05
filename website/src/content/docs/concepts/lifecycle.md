---
title: Lifecycle & Drift Detection
description: How Coral records baselines, reports drift, and merges upstream changes.
---

Coral is built around lifecycle management, not just installation.

The core loop is:

1. install a primitive into a project target
2. record the install-time baseline
3. allow the project to customize the installed artifact
4. detect drift relative to the recorded baseline
5. make that drift visible through listing, diffing, and merge-aware updates

This is the main reason Coral exists. Teams need project-owned primitives that can evolve without
losing provenance.

## How merge works

Coral models every tracked emitted file as three versions:

| Version | Source |
|---|---|
| **Baseline** | Content recorded at last install/update |
| **Local** | Current project-owned artifact |
| **Upstream** | Latest source from the git repository |

The merge behavior depends on which versions changed:

| Local | Upstream | Behavior |
|---|---|---|
| clean | unchanged | No-op |
| clean | changed | Apply upstream, update baseline |
| modified | unchanged | No-op, report local drift |
| modified | changed | Attempt three-way merge with `diffy` |
| conflict | — | Report conflict paths, preserve local files |

### Example: clean update

```sh
$ coral diff python-uv-default --upstream
--- baseline/SKILL.md
+++ upstream/open-agents/SKILL.md
@@ -1,5 +1,5 @@
-# Python UV Default
+# Python UV — Fast Package Manager
 Use uv for Python dependency and environment management.

$ coral update python-uv-default --check
'python-uv-default' can be updated cleanly (no local changes)

$ coral update python-uv-default
installed python-uv-default (open-agents) -> .agents/skills/python-uv-default/SKILL.md
```

### Example: merge conflict

```sh
$ coral update security-review --check
'security-review' has local changes — update would attempt three-way merge

$ coral update security-review
  ✗ index.js: Conflict in "index.js" (lines 12-18)
    <<<<<< local
    severity: string
    ======
    severity: { type: string, enum: [low, medium, high, critical] }
    >>>>>> upstream

  To write conflict markers: coral update security-review --write-conflicts

$ coral update security-review --force   # discard local, apply upstream
installed security-review (claude) -> .claude/tools/security-review/index.js
```

### Commands

| Command | Purpose |
|---|---|
| `coral diff <id>` | Local changes against baseline |
| `coral diff <id> --upstream` | Upstream changes since baseline |
| `coral update <id> --check` | Dry run — show what would happen |
| `coral update <id>` | Attempt three-way merge |
| `coral update <id> --force` | Overwrite local with upstream |

Upstream diff and merge updates require the primitive to be installed from a git source.
