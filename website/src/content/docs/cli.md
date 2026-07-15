---
title: CLI Reference
description: Command reference for the Coral CLI.
---

Run commands from the repository root unless `--global` is specified.

## `coral`

Show the ASCII banner and quick-start menu:

```sh frame="terminal"
coral
```

## `coral init`

Initialize Coral state in the current directory:

```sh frame="terminal"
coral init
```

Initialize global scope (for primitives shared across all projects):

```sh frame="terminal"
coral init --global
```

Creates `.coral/coral-lock.json` (or `~/.coral/coral-lock.json` for global).

## `coral add`

Install a capability from a local directory:

```sh frame="terminal"
# Skill
coral add ./my-skill -t open-agents

# Tool
coral add ./my-tool -t claude

# Multiple targets
coral add ./my-skill -t claude -t open-agents

# Global scope
coral add ./my-skill -t open-agents --global
```

Install a skill from a git repository:

```sh frame="terminal"
coral add https://github.com/owner/repo --skill <name> -t open-agents
```

Install a tool from a git repository:

```sh frame="terminal"
coral add https://github.com/owner/repo --tool <name> -t claude
```

Install a hook from a git repository:

```sh frame="terminal"
coral add https://github.com/owner/repo --hook <name> -t open-agents
```

### Flags

| Flag | Description |
|---|---|
| `-t, --target <id>` | Target harness (required, repeatable) |
| `-s, --skill <name>` | Skill name for git URLs |
| `--tool <name>` | Tool name for git URLs |
| `--hook <name>` | Hook name for git URLs |
| `-g, --global` | Install to global scope (`~/.coral/`) |

## `coral list`

Show installed capabilities with scope, drift status, and path:

```sh frame="terminal"
coral list
```

### Filters

```sh frame="terminal"
# By scope
coral list --scope project
coral list --scope global

# By capability type
coral list --type skill
coral list --type tool

# Combine filters
coral list --scope global --type tool
```

### Status values

| Status | Meaning |
|---|---|
| `clean` | Installed content matches recorded hash |
| `modified` | Installed content has local changes |
| `missing` | Installed file no longer exists |

`coral list` uses terminal colors when supported: clean is green, modified is amber, and missing is red.

## `coral status`

Show per-primitive detail including scope, drift, and override warnings:

```sh frame="terminal"
coral status
```

Example output:

```
python-uv-default  project  clean  [overrides global: won't receive global updates]
commit-hygiene     global   clean
scan-tool          project  clean
```

## `coral outdated`

Show all installed capabilitys and whether upstream updates are available.
Read-only; never modifies files.

```sh frame="terminal"
coral outdated
```

Example output:

```
find-skills              skill      open-agents  2adcfe5    def5678    outdated
pre-commit-lint          hook       open-agents  1.0.0      none       up to date
security-review          tool       claude       abc1234    2adcfe5    outdated
```

For git-sourced primitives, `CURRENT` and `LATEST` show the 7-character commit SHA.
For local primitives, `LATEST` shows `none` and status is always `up to date` or `modified source`.

## `coral diff`

Show unified diff between baseline and installed files, or compare against latest upstream:

```sh frame="terminal"
# Local changes against baseline
coral diff <id>

# Upstream changes since last install (git-sourced only)
coral diff <id> --upstream

# Diff a specific target
coral diff <id> -t claude
```

If the local drift is intentional and should become the new tracked baseline, re-import the
directory with `--override`:

```sh frame="terminal"
coral import .agents/skills/my-skill -t open-agents --override
```

## `coral remove`

Remove a primitive and clean up emitted files:

```sh frame="terminal"
# Remove from project scope (default)
coral remove <id>

# Remove from global scope
coral remove <id> --scope global

# Remove from specific targets only
coral remove <id> -t claude
```

For tools, this also cleans the MCP configuration entry.

## `coral update`

Update a git-sourced primitive to its latest version. Performs a three-way merge
between baseline, local, and upstream. See the [lifecycle docs](/concepts/lifecycle)
for the merge behavior table.

```sh frame="terminal"
# Attempt three-way merge (default)
coral update <id>

# Dry run: show what would happen without applying
coral update <id> --check

# Force overwrite local changes with upstream
coral update <id> --force

# Explicit scope
coral update <id> --scope global
```

## `coral target`

### List available and registered targets

```sh frame="terminal"
coral target list
```

### Register a target

```sh frame="terminal"
coral target add open-agents
coral target add claude
```

Legacy aliases (`codex`, `claude-code`) are accepted and map to the current target names.

### Remove a target

```sh frame="terminal"
coral target remove open-agents
```

Removes all emitted files and MCP registrations for that target across all capabilities.

## `coral import`

Bring existing agent assets under Coral management without rewriting content.

```sh frame="terminal"
# Import a single directory
coral import .agents/skills/my-skill -t open-agents

# Batch scan: imports all skills, tools, and hooks
coral import -t open-agents

# Preview what would be imported
coral import -t open-agents --dry-run

# Overwrite existing lockfile entry
coral import .agents/skills/my-skill -t open-agents --override
```

### Before/after

```
Before import:
.agents/skills/python-uv/
  └── SKILL.md                ← existing, unmanaged

After coral import .agents/skills/python-uv -t open-agents:
.agents/skills/python-uv/
  ├── SKILL.md                ← untouched
  └── coral.toml              ← generated by coral
.coral/
  ├── coral-lock.json         ← entry added
  └── baselines/
    └── open-agents/
      └── python-uv/
        └── SKILL.md          ← baseline copy
```

After import, the directory participates in the full lifecycle. `coral list`,
`coral diff`, `coral check`, and `coral update` all work without modifying
your existing agent files.

:::tip
Batch import scans broadly. `coral import -t open-agents` imports every directory under `.agents/skills/`, `.agents/tools/`, and `.agents/hooks/`. Review with `--dry-run` first.
:::

:::note[Already tracked]
If a directory is already in the lockfile, import skips it with a note. Use `--override` to re-import and overwrite the entry.
:::

:::tip[After import]
Version defaults to `0.1.0`. Edit the generated `coral.toml` to set a real version number and description. The generated manifest lives in your agent directory, so commit it with the rest of your project.
:::

## `coral check`

Validate installed capabilities for CI. Exits 1 on any failure.

```sh frame="terminal"
coral check                    # check all capabilities
coral check --json             # machine-readable JSON output
coral check --ignore-failures  # report failures but exit 0
```

Example output:

```
✓ python-uv-default       skill      open-agents  ok
✗ dirty-skill             skill      open-agents  modified (.agents/skills/dirty-skill/SKILL.md)
```

## CI with GitHub Actions

Add this to your project's `.github/workflows/coral-check.yml`:

```yaml
name: Coral Check
on: [push, pull_request]

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Build and install coral
        run: cargo install --git https://github.com/kannandreams/coral

      - name: Validate capabilities
        run: coral check --json
```

Commit `.coral/coral-lock.json` and `.coral/baselines/` to your repo so `coral check`
runs against the committed state. See the [lockfile reference](/concepts/lockfile) for
what to commit.

## Scope

Coral supports two scopes:

| Scope | Location | Use |
|---|---|---|
| `project` | `.coral/` in repo root | Shared with team via version control |
| `global` | `~/.coral/` in home directory | Available across all projects |

Resolution order: **project always wins**. If the same primitive exists at both scopes,
the project copy shadows the global one. `coral status` flags shadowed primitives.
