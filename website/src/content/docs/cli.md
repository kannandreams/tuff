---
title: CLI Reference
description: Command reference for the Coral CLI.
---

Run commands from the repository root unless `--global` is specified.

New to Coral? Start with the [Getting Started guide](/getting-started), then return here for the
complete command and flag reference.

## Command Groups

| Group | Commands |
|---|---|
| Start | [`coral init`](#coral-init) |
| Create or add capabilities | [`coral create`](#coral-create), [`coral add`](#coral-add) |
| Inspect and generate | [`coral list`](#coral-list), [`coral status`](#coral-status), [`coral generate`](#coral-generate), [`coral outdated`](#coral-outdated) |
| Diff and update | [`coral diff`](#coral-diff), [`coral update`](#coral-update) |
| Validate in CI | [`coral check`](#coral-check) |
| Clean up | [`coral delete`](#coral-delete), [`coral untrack`](#coral-untrack), [`coral cache clear`](#coral-cache-clear) |
| Configure agents and scope | [`coral agent`](#coral-agent), [scope behavior](/concepts/scopes) |

## Start

### `coral`

Show the ASCII banner and quick-start menu:

```sh frame="terminal"
coral
```

### `coral init`

Initialize Coral state in the current directory:

```sh frame="terminal"
coral init
```

Initialize global scope (for primitives shared across all projects):

```sh frame="terminal"
coral init --global
```

Creates `coral.lock` (and a user-state lockfile for global scope),
scaffolds `.agents/`, and configures `open-agents` as the default agent.

## Create or Add Capabilities

### `coral create`

Create and track a new agent-local capability:

```sh frame="terminal"
coral create skill my-skill
coral create tool my-tool -a claude
coral create hook review-hook -a open-agents -a claude
coral create workflow release-flow -a claude
```

The capability type and id are positional. `-a, --agent` is optional and
repeatable. When omitted, Coral uses the configured default agent. Creation
initializes Coral state, registers the selected agents, writes adapter-valid
files, and records the baseline. Use `-a <agent>` when creating for a
different agent.

### `coral add`

Install a capability from a local capability directory or Git URL. The command supports
two forms:

1. Let Coral infer the capability type from a local path.
2. Use an explicit capability-type subcommand when the type is known or when
   installing from a Git repository.

The available capability types are `skill`, `tool`, `hook`, and `workflow`.
In the examples below, `<capability-type>` means “replace this placeholder
with one of those four types.”

#### Local sources

For a local capability directory, Coral can infer the type from its location or
from its manifest:

```sh frame="terminal"
# Auto-detect the type
coral add ./my-skill

# Explicit type
coral add <capability-type> ./path/to/capability

# Explicit type with a selected agent
coral add <capability-type> ./path/to/capability --agent claude

# Multiple agents
coral add <capability-type> ./path/to/capability --agent claude --agent open-agents

# Global scope
coral add <capability-type> ./path/to/capability --global
```

For typed capability commands, `--agent` and `--global` come after the source
and remain scoped to the selected capability type. For example:

```sh frame="terminal"
coral add tool ./my-tool --agent claude
```

For typed capability commands, options come after the capability source:

```sh frame="terminal"
# Agent and scope options stay with the typed command
coral add tool ./my-tool --agent claude --global
```

Add existing agent files in place without copying their content:

```sh frame="terminal"
coral add --agent open-agents .agents/skills/my-skill
coral add --agent claude .agents/skills/my-skill
```

Install from a git repository:

```sh frame="terminal"
coral add <capability-type> https://github.com/owner/repo <name> --agent open-agents
coral add <capability-type> https://github.com/owner/repo <name> --agent claude
```

For Git sources, `<name>` is the capability directory name inside the
repository. For example:

```sh frame="terminal"
coral add skill https://github.com/owner/repo rust-implement --agent open-agents
```

Use the same structure for a tool, hook, or workflow by replacing `skill` with
the corresponding capability type.

For harness-native hooks, pass the hook fragment explicitly:

```sh frame="terminal"
coral add hook ./claude-session-start --agent claude --hook-file settings.json
```

Coral-standard manifest hooks are validated against adapter compatibility and rendered to the
target harness's native settings. To inspect hook support or check a tracked hook before switching
adapters:

```sh frame="terminal"
coral hooks matrix
coral hooks check-portability pre-commit-lint --target claude
```

#### Flags

| Flag | Description |
|---|---|
| `-a, --agent <id>` | Agent harness (optional, repeatable; defaults to configured agent) |
| `-g, --global` | Install to global user scope |
| `--hook-file <path>` | Hook-only native settings fragment, relative to the hook source directory |

The capability type is specified as a subcommand (`skill`, `tool`, `hook`, or
`workflow`) rather than a `--type` flag. For a typed local source, the name is
optional and is normally inferred from the source. For a Git source, the name
is required so Coral knows which capability directory to discover.

### Adding Existing Agent Files

Bring existing agent assets under Coral management without rewriting content:

```sh frame="terminal"
coral add --agent open-agents .agents/skills/python-uv
```

#### Before/after

```text
Before add:
.agents/skills/python-uv/
  └── SKILL.md                ← existing, unmanaged

After coral add --agent open-agents .agents/skills/python-uv:
.agents/skills/python-uv/
  └── SKILL.md                ← untouched
coral.config.json
  ├── coral.lock              ← entry added
  └── objects/
    └── sha256/
      └── a1/
        └── b2c3...           ← immutable baseline object
```

After add, the directory participates in the full lifecycle. `coral list`,
`coral diff`, `coral check`, and `coral update` all work without modifying
your existing agent files. Use `coral update <id>` to accept intentional local
edits as the new baseline.

:::tip[After add]
When Coral adopts existing agent files, it records their hashes as baselines in the lockfile. No additional files are created in the agent directory. The lockfile is the single source of truth for all tracking metadata.
:::

## Inspect and Generate

### `coral list`

Show installed capabilities with scope, drift status, and path:

```sh frame="terminal"
coral list
```

#### Filters

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

#### Status values

| Status | Meaning |
|---|---|
| `clean` | Installed content matches recorded hash |
| `modified` | Installed content has local changes |
| `missing` | Installed file no longer exists |

`coral list` uses terminal colors when supported: clean is green, modified is amber, and missing is red.

### `coral status`

Show per-primitive detail including scope, drift, and override warnings:

```sh frame="terminal"
coral status
```

Example output:

```text
python-uv-default  project  clean  [overrides global: won't receive global updates]
commit-hygiene     global   clean
scan-tool          project  clean
```

### `coral generate`

Generate derived Coral artifacts from tracked project state:

```sh frame="terminal"
# Agent-facing capability index
coral generate index -a open-agents
coral generate index -a claude

# Custom index path
coral generate index -a open-agents --output docs/CAPABILITIES.md

# Human-readable project report
coral generate report
coral generate report --output docs/coral-report.md
```

`coral generate index` writes the default index for the selected agent:

| Agent | Default output |
|---|---|
| `open-agents` | `.agents/CAPABILITIES.md` |
| `claude` | `.claude/CAPABILITIES.md` |

The generated index is intended for agent context. Point `AGENTS.md`,
`CLAUDE.md`, or equivalent agent instructions at the generated
`CAPABILITIES.md` file when you want the agent to see a compact inventory of
tracked capabilities.

`coral generate report` writes `coral-report.md` by default.
The report includes installed capabilities, agents, source type, emitted paths,
and clean/modified/missing status summaries.

Generated files are derived output. The source of truth is
`coral.lock` tracking state.

### `coral outdated`

Show all installed capabilities and whether upstream updates are available.
Read-only; never modifies files.

```sh frame="terminal"
coral outdated
```

Example output:

```text
find-skills              skill      open-agents  2adcfe5    def5678    outdated
pre-commit-lint          hook       open-agents  1.0.0      none       up to date
security-review          tool       claude       abc1234    2adcfe5    outdated
```

For git-sourced primitives, `CURRENT` and `LATEST` show the 7-character commit SHA.
For local primitives, `LATEST` shows `none` and status is always `up to date` or `modified source`.

## Diff and Update

### `coral diff`

Show unified diff between baseline and installed files, or compare against latest upstream:

```sh frame="terminal"
# Local changes against baseline
coral diff <id>

# Upstream changes since last install (git-sourced only)
coral diff <id> --upstream

# Diff a specific agent
coral diff <id> -a claude
```

When color is enabled, diff headers are cyan, additions are green, and deletions are red,
matching the usual Git diff convention. Set `NO_COLOR=1` for plain output.

### `coral update`

Update a capability according to its recorded source. In-place local capabilities accept
current edits as the new baseline; external local sources reload from `sourcePath`;
Git-sourced capabilities perform a three-way merge between baseline, local, and upstream.
See the [lifecycle docs](/concepts/lifecycle) for the merge behavior table.

```sh frame="terminal"
# Update the configured default agent
coral update <id>

# Dry run: show what would happen without applying
coral update <id> --check

# Update a specific agent instead
coral update <id> -a <agent>

# Force overwrite local changes with recorded source output
coral update <id> --force

# Explicit scope
coral update <id> --scope global
```

## Validate in CI

### `coral check`

Validate installed capabilities for CI. Exits 1 on any failure.

```sh frame="terminal"
coral check                    # check all capabilities
coral check --json             # machine-readable JSON output
coral check --ignore-failures  # report failures but exit 0
```

Example output:

```text
✓ python-uv-default       skill      open-agents  ok
✗ dirty-skill             skill      open-agents  modified (.agents/skills/dirty-skill/SKILL.md)
```

### CI with GitHub Actions

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

Commit `coral.lock` to your repo so
`coral check` runs against the committed state. See the [lockfile reference](/concepts/lockfile)
for what to commit.

## Clean Up

### `coral delete`

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

When `-a/--agent` is omitted, `delete` uses the configured agent. It removes emitted files, their baselines,
and generated tool MCP entries. It never deletes the original capability source
directory. Modified generated files require `--force`. In-place added capabilities
cannot be deleted; use `coral untrack` instead.

### `coral untrack`

Stop tracking a capability for explicitly selected agents while preserving its
agent files and manifest:

```sh frame="terminal"
# Stop tracking an in-place added skill for the default agent
coral untrack my-skill

# Stop tracking several agents
coral untrack my-skill -a open-agents -a claude

# Stop tracking a global capability
coral untrack my-skill -a open-agents --scope global
```

`untrack` removes the selected lockfile entry and baseline. It preserves the
capability files, source directories, and MCP configuration.
The lockfile itself remains in place, even when it contains no capabilities.

### `coral cache clear`

Delete Coral's disposable machine-local cache of materialized trees and source
clones. This does not remove project capability files or lockfile entries:

```sh frame="terminal"
coral cache clear
```

## Configure Agents and Scope

### `coral agent`

#### Configure the default agent

```sh frame="terminal"
# Project default
coral agent set-default open-agents

# Global default
coral agent set-default claude --global
```

Commands that accept `-a/--agent` use this value when the flag is omitted.
An explicit agent flag always overrides the default, and repeated flags still
apply an operation to multiple agents.

#### List available and registered agents

```sh frame="terminal"
coral agent list

# Show the global default
coral agent list --global
```

#### Register an agent

```sh frame="terminal"
coral agent add open-agents
coral agent add claude
coral agent add codex
coral agent add cursor
```

Registering an agent also creates its project directory (`.agents/` or
`.claude/`) if it does not already exist.

`claude-code` remains an alias for `claude`. `codex` and `cursor` are dedicated adapter IDs.

The `REGISTERED` column shows which agents are available for Coral operations in
the selected config. The `DEFAULT` column shows which registered agent is used
when `-a/--agent` is omitted.

#### Remove an agent

```sh frame="terminal"
coral agent remove open-agents
```

Unregisters the agent from the project configuration. It does not delete
capabilities, emitted files, baselines, MCP registrations, or lockfile entries.
Use `coral delete <id>` or `coral untrack <id>` for the configured default
agent. Pass `-a <agent>` when selecting a different agent.

### Scope

Coral supports two scopes:

| Scope | Location | Use |
|---|---|---|
| `project` | `coral.lock` in repo root | Shared with team via version control |
| `global` | Coral user state directory | Available across all projects |

Resolution order: **project always wins**. If the same primitive exists at both scopes,
the project copy shadows the global one. `coral status` flags shadowed primitives.
