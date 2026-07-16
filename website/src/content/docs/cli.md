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

Creates `.coral/coral-lock.json` (or `~/.coral/coral-lock.json` for global),
scaffolds `.agents/`, and registers `open-agents` as the default project agent.

## `coral add`

Install a capability from a local directory:

```sh frame="terminal"
# Skill
coral add ./my-skill -a open-agents

# Tool
coral add ./my-tool -a claude

# Multiple agents
coral add ./my-skill -a claude -a open-agents

# Global scope
coral add ./my-skill -a open-agents --global
```

Add existing agent files in place:

```sh frame="terminal"
coral add .agents/skills/my-skill -a open-agents
coral add .claude/skills/my-skill -a claude
```

Install a skill from a git repository:

```sh frame="terminal"
coral add https://github.com/owner/repo --skill <name> -a open-agents
```

Install a tool from a git repository:

```sh frame="terminal"
coral add https://github.com/owner/repo --tool <name> -a claude
```

Install a hook from a git repository:

```sh frame="terminal"
coral add https://github.com/owner/repo --hook <name> -a open-agents
```

### Flags

| Flag | Description |
|---|---|
| `-t, --agent <id>` | Agent harness (required, repeatable) |
| `-s, --skill <name>` | Skill name for git URLs |
| `--tool <name>` | Tool name for git URLs |
| `--hook <name>` | Hook name for git URLs |
| `-g, --global` | Install to global scope (`~/.coral/`) |

## `coral create`

Create and track a new agent-local capability:

```sh frame="terminal"
coral create skill my-skill
coral create tool my-tool -a claude
coral create hook review-hook -a open-agents -a claude
coral create workflow release-flow -a claude
```

The capability type and id are positional. `-t, --agent` is repeatable and
defaults to `open-agents`. Creation initializes Coral state, registers the
selected agents, writes adapter-valid files, and records the baseline. Use
`coral add <path> -a <agent>` for agent files created outside Coral.

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

# Diff a specific agent
coral diff <id> -a claude
```

If the local drift is intentional and should become the new tracked baseline, accept it with
`coral update`:

```sh frame="terminal"
coral update my-skill

# Accept changes for one agent only
coral update my-skill -a open-agents
```

When color is enabled, diff headers are cyan, additions are green, and deletions are red,
matching the usual Git diff convention. Set `NO_COLOR=1` for plain output.

## `coral delete`

Delete Coral-generated capability files for explicitly selected agents:

```sh frame="terminal"
# Delete generated files for one agent
coral delete <id> -a open-agents

# Delete generated files for multiple agents
coral delete <id> -a open-agents -a claude

# Delete from global scope
coral delete <id> -a open-agents --scope global

# Delete files with local modifications
coral delete <id> -a open-agents --force
```

The agent flag is required. `delete` removes emitted files, their baselines,
and generated tool MCP entries. It never deletes the original capability source
directory. Modified generated files require `--force`. In-place added capabilities
cannot be deleted; use `coral untrack` instead.

## `coral untrack`

Stop tracking a capability for explicitly selected agents while preserving its
agent files and manifest:

```sh frame="terminal"
# Stop tracking an in-place added skill
coral untrack my-skill -a open-agents

# Stop tracking several agents
coral untrack my-skill -a open-agents -a claude

# Stop tracking a global capability
coral untrack my-skill -a open-agents --scope global
```

`untrack` removes the selected lockfile entry and baseline. It preserves the
capability files, `coral.toml`, source directories, and MCP configuration.
The lockfile itself remains in place, even when it contains no capabilities.

## `coral update`

Update a capability according to its recorded source. In-place local capabilities accept
current edits as the new baseline; external local sources reload from `sourcePath`;
Git-sourced capabilities perform a three-way merge between baseline, local, and upstream.
See the [lifecycle docs](/concepts/lifecycle) for the merge behavior table.

```sh frame="terminal"
# Attempt three-way merge (default)
coral update <id>

# Dry run: show what would happen without applying
coral update <id> --check

# Update one agent (defaults to all recorded agents)
coral update <id> --agent <agent>

# Force overwrite local changes with recorded source output
coral update <id> --force

# Explicit scope
coral update <id> --scope global
```

## `coral agent`

### List available and registered agents

```sh frame="terminal"
coral agent list
```

### Register an agent

```sh frame="terminal"
coral agent add open-agents
coral agent add claude
```

Registering an agent also creates its project directory (`.agents/` or
`.claude/`) if it does not already exist.

Legacy aliases (`codex`, `claude-code`) are accepted and map to the current agent names.

The `*` marker means the agent is registered; the legend is shown below the table.

### Remove an agent

```sh frame="terminal"
coral agent remove open-agents
```

Unregisters the agent from the project configuration. It does not delete
capabilities, emitted files, baselines, MCP registrations, or lockfile entries.
Use `coral delete <id> -a <agent>` or `coral untrack <id> -a <agent>` for
capability cleanup.

## Adding Existing Agent Files

Bring existing agent assets under Coral management without rewriting content:

```sh frame="terminal"
coral add .agents/skills/python-uv -a open-agents
```

### Before/after

```
Before add:
.agents/skills/python-uv/
  └── SKILL.md                ← existing, unmanaged

After coral add .agents/skills/python-uv -a open-agents:
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

After add, the directory participates in the full lifecycle. `coral list`,
`coral diff`, `coral check`, and `coral update` all work without modifying
your existing agent files. Use `coral update <id>` to accept intentional local
edits as the new baseline.

:::tip[After add]
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
