---
title: Diffing & Updates
description: How Coral shows local drift, accepts local baselines, and updates git-sourced capabilities.
---

Coral uses the recorded baseline in `.coral/baselines/` as the reference point for both local
diffing and upstream-aware updates.

There are three distinct flows:

- local drift against the recorded baseline
- local source changes for capabilities added from an external folder
- upstream changes for git-sourced capabilities

## 1. Diff local changes

Use this when a tracked file in `.agents/` or `.claude/` was edited in the repo.

```sh frame="terminal"
coral list
coral diff my-skill
```

Command behavior:

- `coral list` shows whether the installed files are `clean`, `modified`, or `missing`
- `coral diff <id>` shows the unified diff between the current file and the recorded baseline

### Example

```sh frame="terminal"
coral diff python-project
--- baseline/open-agents/python-project/
+++ .agents/skills/python-project/SKILL.md
@@ -1,2 +1,3 @@
 # Python Project
 Use uv, ruff for linting.
+Always run tests before pushing.
```

If that local change is now the new source of truth, accept it as the new baseline:

```sh frame="terminal"
coral update python-project
```

## 2. Diff upstream changes

Use this when the capability was installed from a git source and you want to inspect what changed
upstream since the last recorded baseline.

```sh frame="terminal"
coral outdated
coral diff rust-implement --upstream
```

### Example

```sh frame="terminal"
coral outdated

coral diff rust-implement --upstream
--- baseline/SKILL.md
+++ upstream/open-agents/SKILL.md
@@ -1,4 +1,5 @@
 # Rust Implement
 Follow the project conventions.
+Run clippy before opening a PR.
```

If nothing changed upstream, Coral prints:

```sh frame="terminal"
no upstream changes
```

## 3. Preview an update

Use `--check` before updating. For in-place local capabilities it previews baseline promotion;
for external local and git-sourced capabilities it previews source reconciliation:

```sh frame="terminal"
coral update rust-implement --check
```

You will see one of these outcomes:

- `'rust-implement' can be updated cleanly (no local changes)`
- `'rust-implement' has local changes: update would attempt a three-way merge`
- `'rust-implement' is up to date`

## 4. Apply an update

```sh frame="terminal"
coral update rust-implement
```

Current update behavior:

- for local capabilities, Coral records the current files as the new baseline
- for local capabilities added from an external source folder, Coral reloads from `sourcePath`
- if local matches baseline and upstream changed, Coral applies upstream
- if upstream matches baseline, Coral leaves local changes alone
- if both local and upstream changed, Coral attempts a three-way merge
- if merge conflicts remain, Coral reports them and keeps local files in place

If you want to discard local changes and take upstream as-is:

```sh frame="terminal"
coral update rust-implement --force
```

## Command reference

| Command | Purpose |
|---|---|
| `coral diff <id>` | Local file changes against baseline |
| `coral diff <id> --upstream` | Upstream changes against baseline |
| `coral outdated` | Show whether git-sourced capabilities have newer upstream commits |
| `coral update <id> --check` | Preview update behavior |
| `coral update <id> -t <target>` | Update one recorded target |
| `coral update <id>` | Apply update using baseline-aware merge logic |
| `coral update <id> --force` | Replace local files with recorded source output |

## Important distinction

Use `coral update` for local baseline promotion, external local source refreshes, and git-backed
upstream reconciliation.
