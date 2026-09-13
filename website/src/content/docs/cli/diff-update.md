---
title: Diff and Update
description: Compare installed capabilities with their baseline or upstream, and move them forward.
---

This page lists the commands and flags. See [Diffing & Updates](/concepts/diff-update) for the
behavior behind them.

## `tuff diff`

Show unified diff between baseline and installed files, or compare against latest upstream:

```sh frame="terminal"
# Local changes against baseline
tuff diff <id>

# Upstream changes since last install (git-sourced only)
tuff diff <id> --upstream

# Diff a specific agent
tuff diff <id> -a claude

# Machine-readable: one object per agent listing changed files
tuff diff <id> --json

# Preview what a different release requirement would change
tuff diff <id>@^2 --upstream
```

`--json` is the same as `--format json`; pass one or the other.

### Capabilities installed at a release

For a capability installed at a [release](/cli/add/#install-a-release), `--upstream` compares against the newest release your recorded requirement allows, which is the same content `tuff update` would install, never the latest commit.

- An exact pin therefore reports `no upstream changes in v1.2.0`, since nothing newer is allowed.
- `tuff diff <id>@<requirement> --upstream` previews a different requirement. For example, `@^2` shows what lifting the pin to the next major would change before `tuff update <id>@^2` does it.
- A note on standard error names the release compared against, and the JSON carries it as `upstream`, so the patch on standard output stays a patch.

A capability pinned to a commit rather than a release still compares against the latest commit.

### Color

When standard output is a terminal, diff headers are cyan, additions are green, and deletions are red, matching the usual Git diff convention. Piped or captured output is plain automatically; set `NO_COLOR=1` to disable color explicitly.

## `tuff update`

Update a capability according to its recorded source:

| Source | What `update` does |
|---|---|
| In-place local capability | Accepts current edits as the new baseline |
| External local source | Reloads from `sourcePath` |
| Git source | Three-way merge between baseline, local, and upstream |

See the [lifecycle docs](/concepts/lifecycle) for the merge behavior table.

```sh frame="terminal"
# Update the configured default agent
tuff update <id>

# Dry run: show what would happen without applying
tuff update <id> --check

# Update a specific agent instead
tuff update <id> -a <agent>

# Force overwrite local changes with recorded source output
tuff update <id> --force

# Explicit scope
tuff update <id> --scope global

# Change the release requirement of a tag-pinned capability, then move
tuff update <id>@^2
```

### Capabilities installed at a release

A capability installed at a [release](/cli/add/#install-a-release) never moves to the latest commit.

- `tuff update <id>` moves to the newest release the recorded requirement allows, and says so when that release is already installed.
- `tuff update <id>@<requirement>` records a new requirement and moves to the newest release it allows. This is how an exact pin such as `1.2.0` is lifted.
- With `--check`, the preview names the release and the claimed size of the change, as in `to 1.4.0 (minor)`, before anything is written.

If the installed tag now names a different commit than when you installed it, `update` refuses rather than calling it up to date. `--check` says what the tag names now, `tuff diff <id> --upstream` shows the difference, and `--force` replaces the install with it.

Capabilities installed from a pack update with the pack; see [Update a pack](/cli/packs/#update-a-pack).
