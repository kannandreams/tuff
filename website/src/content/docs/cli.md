---
title: CLI Reference
description: Command reference for the Tuff CLI.
---

Run commands from the repository root unless `--global` is specified.

New to Tuff? Start with the [Getting Started guide](/getting-started), then return here for the
complete command and flag reference.

## Command Groups

| Group | Commands |
|---|---|
| Start | [`tuff init`](#tuff-init) |
| Create or add capabilities | [`tuff create`](#tuff-create), [`tuff add`](#tuff-add) |
| Inspect and generate | [`tuff list`](#tuff-list), [`tuff status`](#tuff-status), [`tuff generate`](#tuff-generate), [`tuff outdated`](#tuff-outdated) |
| Diff and update | [`tuff diff`](#tuff-diff), [`tuff update`](#tuff-update) |
| Validate in CI | [`tuff check`](#tuff-check) |
| Clean up | [`tuff delete`](#tuff-delete), [`tuff untrack`](#tuff-untrack), [`tuff cache clear`](#tuff-cache-clear) |
| Configure agents and scope | [`tuff agent`](#tuff-agent), [scope behavior](/concepts/scopes) |

## Start

### `tuff`

Show the ASCII banner and quick-start menu:

```sh frame="terminal"
tuff
```

### `tuff init`

Initialize Tuff state in the current directory:

```sh frame="terminal"
tuff init
```

Initialize global scope (for primitives shared across all projects):

```sh frame="terminal"
tuff init --global
```

Creates `tuff.lock` (and a user-state lockfile for global scope),
scaffolds `.agents/`, and configures `open-agents` as the default agent.

## Create or Add Capabilities

### `tuff create`

Create and track a new agent-local capability:

```sh frame="terminal"
tuff create skill my-skill
tuff create tool my-tool -a claude
tuff create hook review-hook -a open-agents -a claude
tuff create workflow release-flow -a claude
```

The capability type and id are positional. `-a, --agent` is optional and
repeatable. When omitted, Tuff uses the configured default agent. Creation
initializes Tuff state, registers the selected agents, writes adapter-valid
files, and records the baseline. Use `-a <agent>` when creating for a
different agent.

### `tuff add`

Install a capability from a local capability directory or Git URL. The command supports
two forms:

1. Let Tuff infer the capability type from a local path.
2. Use an explicit capability-type subcommand when the type is known or when
   installing from a Git repository.

The available capability types are `skill`, `tool`, `hook`, and `workflow`.
In the examples below, `<capability-type>` means “replace this placeholder
with one of those four types.”

#### Local sources

For a local capability directory, Tuff can infer the type from its location or
from its manifest:

```sh frame="terminal"
# Auto-detect the type
tuff add ./my-skill

# Explicit type
tuff add <capability-type> ./path/to/capability

# Explicit type with a selected agent
tuff add <capability-type> ./path/to/capability --agent claude

# Multiple agents
tuff add <capability-type> ./path/to/capability --agent claude --agent open-agents

# Global scope
tuff add <capability-type> ./path/to/capability --global
```

For typed capability commands, `--agent` and `--global` come after the source
and remain scoped to the selected capability type. For example:

```sh frame="terminal"
tuff add tool ./my-tool --agent claude
```

For typed capability commands, options come after the capability source:

```sh frame="terminal"
# Agent and scope options stay with the typed command
tuff add tool ./my-tool --agent claude --global
```

Add existing agent files in place without copying their content:

```sh frame="terminal"
tuff add --agent open-agents .agents/skills/my-skill
tuff add --agent claude .agents/skills/my-skill
```

Install from a git repository:

```sh frame="terminal"
tuff add <capability-type> https://github.com/owner/repo <name> --agent open-agents
tuff add <capability-type> https://github.com/owner/repo <name> --agent claude
```

For Git sources, `<name>` is the capability directory name inside the
repository. For example:

```sh frame="terminal"
tuff add skill https://github.com/owner/repo rust-implement --agent open-agents
```

Use the same structure for a tool, hook, or workflow by replacing `skill` with
the corresponding capability type.

For harness-native hooks, pass the hook fragment explicitly:

```sh frame="terminal"
tuff add hook ./claude-session-start --agent claude --hook-file settings.json
```

Tuff-standard manifest hooks are validated against adapter compatibility and rendered to the
target harness's native settings. To inspect hook support or check a tracked hook before switching
adapters:

```sh frame="terminal"
tuff hooks matrix
tuff hooks check-portability pre-commit-lint --target claude
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
is required so Tuff knows which capability directory to discover.

### Adding Existing Agent Files

Bring existing agent assets under Tuff management without rewriting content:

```sh frame="terminal"
tuff add --agent open-agents .agents/skills/python-uv
```

#### Before/after

```text
Before add:
.agents/skills/python-uv/
  └── SKILL.md                ← existing, unmanaged

After tuff add --agent open-agents .agents/skills/python-uv:
.agents/skills/python-uv/
  └── SKILL.md                ← untouched
tuff.config.json
  ├── tuff.lock              ← entry added
  └── objects/
    └── sha256/
      └── a1/
        └── b2c3...           ← immutable baseline object
```

After add, the directory participates in the full lifecycle. `tuff list`,
`tuff diff`, `tuff check`, and `tuff update` all work without modifying
your existing agent files. Use `tuff update <id>` to accept intentional local
edits as the new baseline.

:::tip[After add]
When Tuff adopts existing agent files, it records their hashes as baselines in the lockfile. No additional files are created in the agent directory. The lockfile is the single source of truth for all tracking metadata.
:::

## Inspect and Generate

### `tuff list`

Show installed capabilities with scope, drift status, and path:

```sh frame="terminal"
tuff list
```

#### Filters

```sh frame="terminal"
# By scope
tuff list --scope project
tuff list --scope global

# By capability type
tuff list --type skill
tuff list --type tool

# Combine filters
tuff list --scope global --type tool
```

#### Status values

| Status | Meaning |
|---|---|
| `clean` | Installed content matches recorded hash |
| `modified` | Installed content has local changes |
| `missing` | Installed file no longer exists |

`tuff list` uses terminal colors when supported: clean is green, modified is amber, and missing is red.

### `tuff status`

Show per-primitive detail including scope, drift, and override warnings:

```sh frame="terminal"
tuff status
```

Example output:

```text
python-uv-default  project  clean  [overrides global: won't receive global updates]
commit-hygiene     global   clean
scan-tool          project  clean
```

### `tuff generate`

Generate derived Tuff artifacts from tracked project state:

```sh frame="terminal"
# Agent-facing capability index
tuff generate index -a open-agents
tuff generate index -a claude

# Custom index path
tuff generate index -a open-agents --output docs/CAPABILITIES.md

# Human-readable project report
tuff generate report
tuff generate report --output docs/tuff-report.md
```

`tuff generate index` writes the default index for the selected agent:

| Agent | Default output |
|---|---|
| `open-agents` | `.agents/CAPABILITIES.md` |
| `claude` | `.claude/CAPABILITIES.md` |

The generated index is intended for agent context. Point `AGENTS.md`,
`CLAUDE.md`, or equivalent agent instructions at the generated
`CAPABILITIES.md` file when you want the agent to see a compact inventory of
tracked capabilities.

`tuff generate report` writes `tuff-report.md` by default.
The report includes installed capabilities, agents, source type, emitted paths,
and clean/modified/missing status summaries.

Generated files are derived output. The source of truth is
`tuff.lock` tracking state.

### `tuff outdated`

Show all installed capabilities and whether upstream updates are available.
Read-only; never modifies files.

```sh frame="terminal"
tuff outdated
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

### `tuff diff`

Show unified diff between baseline and installed files, or compare against latest upstream:

```sh frame="terminal"
# Local changes against baseline
tuff diff <id>

# Upstream changes since last install (git-sourced only)
tuff diff <id> --upstream

# Diff a specific agent
tuff diff <id> -a claude
```

When color is enabled, diff headers are cyan, additions are green, and deletions are red,
matching the usual Git diff convention. Set `NO_COLOR=1` for plain output.

### `tuff update`

Update a capability according to its recorded source. In-place local capabilities accept
current edits as the new baseline; external local sources reload from `sourcePath`;
Git-sourced capabilities perform a three-way merge between baseline, local, and upstream.
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
```

## Validate in CI

### `tuff check`

Validate installed capabilities for CI. Exits 1 on any failure.

```sh frame="terminal"
tuff check                    # check all capabilities
tuff check --json             # machine-readable JSON output
tuff check --ignore-failures  # report failures but exit 0
```

Example output:

```text
✓ python-uv-default       skill      open-agents  ok
✗ dirty-skill             skill      open-agents  modified (.agents/skills/dirty-skill/SKILL.md)
```

### CI with GitHub Actions

Add this to your project's `.github/workflows/tuff-check.yml`:

```yaml
name: Tuff Check
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

      - name: Build and install tuff
        run: cargo install tuffcli

      - name: Validate capabilities
        run: tuff check --json
```

Commit `tuff.lock` to your repo so
`tuff check` runs against the committed state. See the [lockfile reference](/concepts/lockfile)
for what to commit.

## Clean Up

### `tuff delete`

Delete Tuff-generated capability files for explicitly selected agents:

```sh frame="terminal"
# Delete generated files for one agent
tuff delete <id> -a open-agents

# Delete generated files for multiple agents
tuff delete <id> -a open-agents -a claude

# Delete from global scope
tuff delete <id> -a open-agents --scope global

# Delete files with local modifications
tuff delete <id> -a open-agents --force
```

When `-a/--agent` is omitted, `delete` uses the configured agent. It removes emitted files, their baselines,
and generated tool MCP entries. It never deletes the original capability source
directory. Modified generated files require `--force`. In-place added capabilities
cannot be deleted; use `tuff untrack` instead.

### `tuff untrack`

Stop tracking a capability for explicitly selected agents while preserving its
agent files and manifest:

```sh frame="terminal"
# Stop tracking an in-place added skill for the default agent
tuff untrack my-skill

# Stop tracking several agents
tuff untrack my-skill -a open-agents -a claude

# Stop tracking a global capability
tuff untrack my-skill -a open-agents --scope global
```

`untrack` removes the selected lockfile entry and baseline. It preserves the
capability files, source directories, and MCP configuration.
The lockfile itself remains in place, even when it contains no capabilities.

### `tuff cache clear`

Delete Tuff's disposable machine-local cache of materialized trees and source
clones. This does not remove project capability files or lockfile entries:

```sh frame="terminal"
tuff cache clear
```

## Configure Agents and Scope

### `tuff agent`

#### Configure the default agent

```sh frame="terminal"
# Project default
tuff agent set-default open-agents

# Global default
tuff agent set-default claude --global
```

Commands that accept `-a/--agent` use this value when the flag is omitted.
An explicit agent flag always overrides the default, and repeated flags still
apply an operation to multiple agents.

#### List available and registered agents

```sh frame="terminal"
tuff agent list

# Show the global default
tuff agent list --global
```

#### Register an agent

```sh frame="terminal"
tuff agent add open-agents
tuff agent add claude
tuff agent add codex
tuff agent add cursor
```

Registering an agent also creates its project directory (`.agents/` or
`.claude/`) if it does not already exist.

`claude-code` remains an alias for `claude`. `codex` and `cursor` are dedicated adapter IDs.

The `REGISTERED` column shows which agents are available for Tuff operations in
the selected config. The `DEFAULT` column shows which registered agent is used
when `-a/--agent` is omitted.

#### Remove an agent

```sh frame="terminal"
tuff agent remove open-agents
```

Unregisters the agent from the project configuration. It does not delete
capabilities, emitted files, baselines, MCP registrations, or lockfile entries.
Use `tuff delete <id>` or `tuff untrack <id>` for the configured default
agent. Pass `-a <agent>` when selecting a different agent.

### Scope

Tuff supports two scopes:

| Scope | Location | Use |
|---|---|---|
| `project` | `tuff.lock` in repo root | Shared with team via version control |
| `global` | Tuff user state directory | Available across all projects |

Resolution order: **project always wins**. If the same primitive exists at both scopes,
the project copy shadows the global one. `tuff status` flags shadowed primitives.
