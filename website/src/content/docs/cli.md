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
| Build and deliver packs | [`tuff pack`](#tuff-pack), [`tuff add pack`](#install-a-pack) |
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

Override the installed ID for a local source with `--name`; the overridden ID is used for emitted paths and the lockfile entry:

```sh frame="terminal"
tuff add ./path/to/capability --name team-capability --agent open-agents
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

#### Install a pack

Install every member of a verified local pack artifact into project scope:

```sh frame="terminal"
tuff init
tuff add pack ./tuff-dist/crm-integration-1.2.0.tuffpack --agent open-agents
```

Pack installation verifies the complete artifact, preflights every member, stages shared hook and MCP configuration, and refuses the entire installation if any member is already tracked or would overwrite an untracked file. `--agent` is optional and repeatable; pack installation does not support `--global`.

If the pack came from a registry, pass `--reference` with the reference you
pulled it from so `tuff outdated` can check for a newer version later:

```sh frame="terminal"
tuff pack pull ghcr.io/acme/engineering:1.2.0 --output ./engineering.tuffpack
tuff add pack ./engineering.tuffpack --agent open-agents \
  --reference ghcr.io/acme/engineering:1.2.0
```

`tuff add pack` only ever sees the local artifact file; it has no way to know
where it came from unless told. Without `--reference`, `tuff outdated` reports
this pack's capabilities as `not checked` rather than guessing.

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
| `-n, --name <id>` | Override the installed capability ID for an auto-detected local source |
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

## Build and Deliver Packs

New to packs? Start with the [Tuff Pack examples repository](https://github.com/kannandreams/tuff-pack-examples). It follows tracked capabilities through build, inspection, GHCR publication, pull, extraction, and a container image before introducing the lower-level details.

### `tuff pack`

Package all project-scoped tracked capabilities except `tuff-cli-guide`, using version `0.1.0` and the configured default agent:

```sh frame="terminal"
tuff pack build --name crm-integration
```

The default output is `tuff-dist/crm-integration-0.1.0.tuffpack`. Select capabilities, targets, a version, or a different output when needed:

```sh frame="terminal"
tuff pack build --name crm-integration --version 1.2.0 \
  --capability crm-operating --capability lead-triage \
  --agent open-agents --agent claude \
  --output releases/crm-integration-1.2.0.tuffpack
```

Create a reusable project-backed definition under `tuff-packs/<name>/tuff-pack.toml` without copying capability files:

```sh frame="terminal"
tuff pack init crm-integration --from-project \
  --capability crm-operating --capability lead-triage
tuff pack check tuff-packs/crm-integration
tuff pack build tuff-packs/crm-integration
```

Selecting a workflow automatically includes its tracked transitive requirements. Project builds fail when selected capability files or sources differ from the accepted `tuff.lock` baseline; accept intentional changes with `tuff update <capability>` first.

Standalone path-based source packs remain supported. Both commands default to the current directory:

```sh frame="terminal"
tuff pack check [path]
tuff pack build [path] --output <artifact.tuffpack>
```

For a standalone pack, omitting `--output` writes `<pack-name>-<pack-version>.tuffpack` beneath the pack root. Build always refuses to overwrite an existing artifact.

Inspect or verify an artifact:

```sh frame="terminal"
tuff pack inspect <artifact.tuffpack>
tuff pack inspect <artifact.tuffpack> --json
tuff pack verify <artifact.tuffpack>
```

Publish a verified artifact to an OCI registry, or pull it back by an explicit tag or digest:

```sh frame="terminal"
tuff pack push <artifact.tuffpack> ghcr.io/yourorg/crm-integration:1.2.0
tuff pack pull ghcr.io/yourorg/crm-integration:1.2.0 --output crm-integration-1.2.0.tuffpack
tuff pack pull ghcr.io/yourorg/crm-integration@sha256:<manifest-digest> --output pinned.tuffpack
```

`pack push` refuses to move a tag that already names different content unless `--force` is supplied; pushing identical content is idempotent. `pack pull` resolves a tag to an immutable manifest digest before downloading, verifies the OCI descriptors and the complete Tuff artifact, and refuses to overwrite an existing output file. Both commands accept `--json`, repeatable `--ca-file <pem>`, and `--plain-http` for disposable development registries. Tuff uses existing Docker credentials first, then Podman credentials, and otherwise attempts anonymous access.

Extract one pre-rendered adapter target without creating project lockfile state:

```sh frame="terminal"
tuff pack extract <artifact.tuffpack> --agent <id> --output <directory>
```

The output directory must be missing or empty. See [Capability Packs](/concepts/packs/) for the source manifest, artifact guarantees, and installation behavior.

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
find-skills               skill      open-agents  2adcfe5    def5678    outdated
security-review           tool       claude       abc1234    2adcfe5    outdated
rust-implement            skill      open-agents  1.2.0      1.4.0      outdated
csv-workbench             skill      open-agents  1.0.0      —          not checked
```

For git-sourced capabilities, `CURRENT` and `LATEST` show the 7-character commit SHA.

For a pack-sourced capability installed with `add pack --reference`,
`CURRENT` and `LATEST` compare the *pack's* published versions — the numbers
you passed to `tuff pack build --version` — since that is the artifact
whose availability is actually being checked. Only tags that parse as
[semver](https://semver.org) are compared; anything else is excluded rather
than guessed at, and `1.9.0` is correctly treated as older than `1.10.0`
(plain string comparison would get this backwards). Pass `--plain-http` or
`--ca-file` for a self-hosted registry, matching `tuff pack push`/`pull`.

Anything Tuff cannot check — a pack installed without `--reference`, or a
local capability with no source at all — reports `not checked`, `—`, rather
than a guessed `up to date`.

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

When standard output is a terminal, diff headers are cyan, additions are green, and deletions are red, matching the usual Git diff convention. Piped or captured output is plain automatically; set `NO_COLOR=1` to disable color explicitly.

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
tuff check --global           # check global capabilities only
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
