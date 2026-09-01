---
title: Diffing & Updates
description: How Tuff shows local drift, accepts local baselines, and updates git-sourced capabilities.
---

Tuff uses verified materialized trees from its user cache directory as the reference point for both
local diffing and upstream-aware updates. Diffing uses temporary Git trees through libgit2;
the consuming project does not need to be a Git repository.

This page explains the behavior behind the commands. See the [CLI Reference](/cli#tuff-diff) for
the complete command and flag syntax.

There are three distinct flows:

- local drift against the recorded baseline
- local source changes for capabilities added from an external folder
- upstream changes for git-sourced capabilities

## 1. Diff local changes

Use this when a tracked file in `.agents/` or `.claude/` was edited in the repo.

```sh frame="terminal"
tuff list
tuff diff my-skill
```

Command behavior:

- `tuff list` shows whether the installed files are `clean`, `modified`, or `missing`
- `tuff diff <id>` shows the unified diff between the current file and the recorded baseline

### Example

```sh frame="terminal"
tuff diff python-project
--- baseline/open-agents/python-project/
+++ .agents/skills/python-project/SKILL.md
@@ -1,2 +1,3 @@
 # Python Project
 Use uv, ruff for linting.
+Always run tests before pushing.
```

If that local change is now the new source of truth, accept it as the new baseline:

```sh frame="terminal"
tuff update python-project
```

## 2. Diff upstream changes

Use this when the capability was installed from a git source and you want to inspect what changed
upstream since the last recorded baseline.

```sh frame="terminal"
tuff outdated
tuff diff rust-implement --upstream
```

### Example

```sh frame="terminal"
tuff outdated

tuff diff rust-implement --upstream
--- baseline/SKILL.md
+++ upstream/open-agents/SKILL.md
@@ -1,4 +1,5 @@
 # Rust Implement
 Follow the project conventions.
+Run clippy before opening a PR.
```

If nothing changed upstream, Tuff prints:

```sh frame="terminal"
no upstream changes
```

## 3. Preview an update

Use `--check` before updating. For in-place local capabilities it previews baseline promotion;
for external local and git-sourced capabilities it previews source reconciliation:

```sh frame="terminal"
tuff update rust-implement --check
```

You will see one of these outcomes:

- `'rust-implement' can be updated cleanly (no local changes)`
- `'rust-implement' has local changes: update would attempt a three-way merge`
- `'rust-implement' is up to date`

## 4. Apply an update

```sh frame="terminal"
tuff update rust-implement
```

Current update behavior:

- for local capabilities, Tuff records the current files as the new baseline
- for local capabilities added from an external source folder, Tuff reloads from `sourcePath`
- if local matches baseline and upstream changed, Tuff applies upstream
- if upstream matches baseline, Tuff leaves local changes alone
- if both local and upstream changed, Tuff attempts a three-way merge
- if merge conflicts remain, Tuff reports them and keeps local files in place

If you want to discard local changes and take upstream as-is:

```sh frame="terminal"
tuff update rust-implement --force
```

For a capability installed from a pack, `tuff update <member>` updates the whole pack: it resolves the registry recorded at install time (or takes an artifact with `--pack`), then replaces, adds, and removes members so the project matches the new release. See [Capability Packs](/concepts/packs#update-an-installed-pack).

## Command reference

| Command | Purpose |
|---|---|
| `tuff diff <id>` | Local file changes against baseline |
| `tuff diff <id> --upstream` | Upstream changes against baseline |
| `tuff outdated` | Show whether git-sourced capabilities have newer upstream commits |
| `tuff update <id> --check` | Preview update behavior |
| `tuff update <id>` | Update the configured default agent |
| `tuff update <id> -a <agent>` | Update one explicitly selected agent |
| `tuff update <id> --force` | Replace local files with recorded source output |
| `tuff update <member> --pack <artifact>` | Move a pack-installed capability's whole pack to the release in `<artifact>` |

## Important distinction

Use `tuff update` for local baseline promotion, external local source refreshes, and git-backed
upstream reconciliation.
