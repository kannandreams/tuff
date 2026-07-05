---
title: CLI Reference
description: Command reference for the Coral CLI.
---

Run commands from the repository root unless `--global` is specified.

## `coral`

Show the ASCII banner and quick-start menu:

```sh
coral
```

## `coral init`

Initialize Coral state in the current directory:

```sh
coral init
```

Initialize global scope (for primitives shared across all projects):

```sh
coral init --global
```

Creates `.coral/coral-lock.json` (or `~/.coral/coral-lock.json` for global).

## `coral add`

Install a capability from a local directory:

```sh
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

```sh
coral add https://github.com/owner/repo --skill <name> -t open-agents
```

Install a tool from a git repository:

```sh
coral add https://github.com/owner/repo --tool <name> -t claude
```

Install a hook from a git repository:

```sh
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

Show installed primitives with scope, drift status, and path:

```sh
coral list
```

### Filters

```sh
# By scope
coral list --scope project
coral list --scope global

# By primitive kind
coral list --primitive skill
coral list --primitive tool

# Combine filters
coral list --scope global --primitive tool
```

### Status values

| Status | Meaning |
|---|---|
| `clean` | Installed content matches recorded hash |
| `modified` | Installed content has local changes |
| `missing` | Installed file no longer exists |

## `coral status`

Show per-primitive detail including scope, drift, and override warnings:

```sh
coral status
```

Example output:

```
python-uv-default  project  clean  [overrides global — won't receive global updates]
commit-hygiene     global   clean
scan-tool          project  clean
```

## `coral diff`

Show unified diff between baseline and installed files, or compare against latest upstream:

```sh
# Local changes against baseline
coral diff <id>

# Upstream changes since last install (git-sourced only)
coral diff <id> --upstream

# Diff a specific target
coral diff <id> -t claude
```

## `coral remove`

Remove a primitive and clean up emitted files:

```sh
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

```sh
# Attempt three-way merge (default)
coral update <id>

# Dry run — show what would happen without applying
coral update <id> --check

# Force overwrite local changes with upstream
coral update <id> --force

# Explicit scope
coral update <id> --scope global
```

## `coral target`

### List available and registered targets

```sh
coral target list
```

### Register a target

```sh
coral target add open-agents
coral target add claude
```

Legacy aliases (`codex`, `claude-code`) are accepted and map to the current target names.

### Remove a target

```sh
coral target remove open-agents
```

Removes all emitted files and MCP registrations for that target across all primitives.

## Scope

Coral supports two scopes:

| Scope | Location | Use |
|---|---|---|
| `project` | `.coral/` in repo root | Shared with team via version control |
| `global` | `~/.coral/` in home directory | Available across all projects |

Resolution order: **project always wins**. If the same primitive exists at both scopes,
the project copy shadows the global one. `coral status` flags shadowed primitives.
