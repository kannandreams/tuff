---
title: CLI Reference
description: Every Tuff CLI command, grouped by task, with exit codes and error output for scripts and CI.
---

Run commands from the repository root unless `--global` is specified.

New to Tuff? Start with the [Getting Started guide](/getting-started), then return here for the
complete command and flag reference.

## Command Groups

The reference is split by task. Each page covers its commands, their flags, and examples.

| Page | Commands |
|---|---|
| Start (this page) | [`tuff`](#tuff), [`tuff init`](#tuff-init) |
| [Create and Add](/cli/add/) | [`tuff create`](/cli/add/#tuff-create), [`tuff add`](/cli/add/#tuff-add), [`tuff scan`](/cli/add/#tuff-scan) |
| [Packs](/cli/packs/) | [`tuff pack`](/cli/packs/#tuff-pack), [`tuff add pack`](/cli/packs/#install-a-pack), [pack updates](/cli/packs/#update-a-pack) |
| [MCP Servers](/cli/mcp/) | [`tuff add mcp`](/cli/mcp/#install-an-mcp-server), [`tuff mcp catalog`](/cli/mcp/#tuff-mcp-catalog), [`tuff mcp search`](/cli/mcp/#tuff-mcp-search), [`tuff mcp doctor`](/cli/mcp/#tuff-mcp-doctor) |
| [Inspect and Generate](/cli/inspect/) | [`tuff list`](/cli/inspect/#tuff-list), [`tuff status`](/cli/inspect/#tuff-status), [`tuff generate`](/cli/inspect/#tuff-generate), [`tuff outdated`](/cli/inspect/#tuff-outdated), [`tuff policy matrix`](/cli/inspect/#tuff-policy-matrix) |
| [Diff and Update](/cli/diff-update/) | [`tuff diff`](/cli/diff-update/#tuff-diff), [`tuff update`](/cli/diff-update/#tuff-update) |
| [Validate in CI](/cli/ci/) | [`tuff check`](/cli/ci/#tuff-check), [GitHub Actions](/cli/ci/#ci-with-github-actions) |
| [Clean Up](/cli/clean-up/) | [`tuff delete`](/cli/clean-up/#tuff-delete), [`tuff untrack`](/cli/clean-up/#tuff-untrack), [`tuff lock migrate`](/cli/clean-up/#tuff-lock-migrate), [`tuff cache clear`](/cli/clean-up/#tuff-cache-clear) |
| [Agents and Scope](/cli/agents/) | [`tuff agent`](/cli/agents/#tuff-agent), [scope](/cli/agents/#scope) |

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

Project initialization also looks for the harnesses already present in the repository and registers them:

- A `.claude/` directory or a `CLAUDE.md` file registers `claude`.
- A `.cursor/` directory registers `cursor`.

Each registered harness is given the `tuff-cli-guide` skill in its own layout, so the agent you actually run can read it. Codex is not detected here, because it writes the same `.agents/` root that `open-agents` already covers.

## Exit codes and errors

Every command uses the same exit codes, so a script can branch without reading the message:

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | The operation failed: something was not found, was refused, had local changes, or a source was unreachable |
| `2` | The command was called wrongly: a bad flag or argument value |
| `70` | A bug in Tuff. Worth reporting |

`tuff check` and `tuff mcp doctor` exit `1` when they find a problem, which is what makes them usable as CI gates.

Failures print the message on one line and, when there is a clear next step, a hint on a second:

```text
error: 'rust-implement' has local modifications for agent 'claude'
hint: use --force to delete them
```

`list`, `outdated`, `diff`, `check`, `mcp doctor`, `mcp search`, and `pack inspect` all take `--json`. When the invocation includes it, a failure is reported as one JSON line on stderr instead, so machine-readable output stays machine-readable:

```json
{"error":{"kind":"drift","message":"'rust-implement' has local modifications for agent 'claude'","hint":"use --force to delete them"}}
```

The `kind` is one of `usage`, `not_found`, `refused`, `drift`, `source`, `corrupt`, `unsupported`, `io`, or `internal`.
