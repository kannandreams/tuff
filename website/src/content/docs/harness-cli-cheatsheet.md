---
title: Harness Config
description: A practical command, flag, target-folder, and operations reference for Claude Code, Codex CLI, Cursor CLI, OpenCode, and Coral.
---

This is the quick reference for teams that use more than one coding-agent
harness. It separates two concerns:

1. The harness CLI controls a session: prompt, working directory, model,
   permissions, output, and resume behavior.
2. Coral manages reusable project capabilities: skills, tools, hooks, and
   workflows, then emits them into the selected harness layout.

The upstream CLIs change quickly. Treat this page as a workflow map and run
`<command> --help` for the exact version installed on your machine. The links
in the last section are the authoritative references.

## Quick Comparison

| Harness | Start | Project config |
|---|---|---|
| Claude Code | `claude` | `CLAUDE.md` · `.claude/` |
| Codex CLI | `codex` | `AGENTS.md` · `.agents/` |
| Cursor CLI | `cursor-agent` | `.cursor/rules/` · `.cursor/commands/` |
| OpenCode | `opencode` | `AGENTS.md` · `.opencode/` |

## Target-folder map

Use this map before adding a file manually. A target is project-local unless
the path begins with `~`; project-local files are normally committed, while
local settings and user state should stay out of version control.

### Claude Code

| Intent | Target |
|---|---|
| Shared instructions | `CLAUDE.md` or `.claude/CLAUDE.md` |
| Personal instructions | `CLAUDE.local.md` or `.claude/settings.local.json` |
| Skills and commands | `.claude/skills/<name>/SKILL.md`<br>`.claude/commands/<name>.md` |
| Agents and rules | `.claude/agents/`<br>`.claude/rules/` |
| MCP and hooks | `.mcp.json`<br>`.claude/settings.json` |
| User config | `~/.claude/` and `~/.claude.json` |

### Codex CLI

| Intent | Target |
|---|---|
| Shared instructions | `AGENTS.md` |
| Local override | `AGENTS.override.md` in the relevant directory |
| Skills | `.agents/skills/<name>/SKILL.md` |
| MCP and runtime config | `~/.codex/config.toml` or `CODEX_HOME` |

### Cursor CLI

| Intent | Target |
|---|---|
| Shared rules | `.cursor/rules/<name>.mdc` |
| Reusable commands | `.cursor/commands/<name>.md` |
| CLI permissions | `.cursor/cli.json` or `~/.cursor/cli-config.json` |
| MCP | `mcp.json` |

### OpenCode

| Intent | Target |
|---|---|
| Shared instructions | `AGENTS.md` or project rules in `.opencode/` |
| Skills | `.opencode/skills/<name>/SKILL.md` |
| Commands and agents | `.opencode/commands/`<br>`.opencode/agents/` |
| Project config | `opencode.json` or `opencode.jsonc` |
| User config | `~/.config/opencode/` |

Coral is the cross-harness layer: use `coral.toml` and `coral.lock` as the
source of truth, then emit adapter-specific files into `.agents/`, `.claude/`,
or another supported target.

### What Coral emits

Coral keeps the capability source and lock metadata independent from the
harness output:

```text
coral.toml                 # capability manifest / source declaration
coral.lock                 # tracked identity, scope, agent, and baseline
.agents/                   # open-agents / Codex-compatible output
  skills/<id>/SKILL.md
.claude/                   # Claude-oriented output
  skills/<id>/SKILL.md
  commands/<id>.md
```

The exact emitted paths depend on the capability type and adapter. Inspect
them with `coral list`, `coral status`, or `coral generate report` instead of
assuming that every harness has the same layout.

## Flag translation

The same operational intent has different spellings. Do not copy a flag from
one harness to another without checking its safety model.

| Intent | Claude Code | Codex CLI | Cursor CLI | OpenCode |
|---|---|---|---|---|
| Start interactive | `claude` | `codex` | `cursor-agent` | `opencode` |
| Non-interactive | `-p`, `--print` | `exec` / `e` | `-p`, `--print` | `run` |
| Select working directory | run from directory; `--add-dir` adds access | `--cd`, `-C`; `--add-dir` | run from directory | `--dir` on attach; run from project directory |
| Continue latest | `-c` | `exec resume --last` or interactive resume | `cursor-agent resume` | `--continue`, `-c` |
| Resume by id | `--resume`, `-r` | `exec resume <id>` | `resume [thread id]` | `--session`, `-s` |
| Choose model | `--model` | `--model` / config | `--model` | `--model`, `-m` |
| Add another directory | `--add-dir` | `--add-dir` | use project context / permissions config | use config and project discovery |
| Structured output | `--output-format json` or `stream-json` | `--json` and optional `--output-last-message` | `--output-format json` | `--format json` for `run` |
| Permission posture | `--permission-mode plan` / `acceptEdits`; avoid bypass by default | `--ask-for-approval on-request` plus `--sandbox workspace-write` | approval prompts; headless writes need `--force` | configure `permission`; `--auto` auto-approves |
| Session-only instructions | `--append-system-prompt` or `--settings` | `-c key=value` and prompt | prompt text | `OPENCODE_CONFIG_CONTENT` or prompt |

Safety note: `--dangerously-skip-permissions`, Codex `--yolo`, Cursor
`--force`, and OpenCode `--auto` can materially increase write or command
execution authority. Prefer the narrowest permission mode that completes the
task, especially in CI.

## Common workflows

### 1. Start a task with the correct project context

```sh frame="terminal"
# These examples use terminal frames so they read like a macOS/Linux shell.

# Claude Code
claude --permission-mode plan "Inspect the release workflow and propose changes"

# Codex CLI
codex --cd . --ask-for-approval on-request --sandbox workspace-write \
  "Inspect the release workflow and propose changes"

# Cursor CLI
cursor-agent -p "Inspect the release workflow and propose changes" \
  --output-format text

# OpenCode
opencode run "Inspect the release workflow and propose changes"
```

Expected target: the current repository, its instruction file, and the
harness-specific capability folders discovered from that repository.

### 2. Add a capability with Coral

```sh frame="terminal"
coral init
coral agent add open-agents
coral agent add claude

# Create a new capability for both harnesses
coral create skill release-check -a open-agents -a claude

# Adopt an existing project capability without rewriting its content
coral add --agent open-agents .agents/skills/release-check

# Inspect what is tracked and where it was emitted
coral list
coral status
coral generate report --output docs/coral-report.md
```

Use `coral add skill <source> --agent <id>` when the source type is known, or
the untyped `coral add <source>` form when Coral should infer it. For a
harness-native hook fragment:

```sh frame="terminal"
coral add hook ./claude-session-start --agent claude --hook-file settings.json
```

### 3. Detect and reconcile drift

```sh frame="terminal"
coral list
coral diff <capability-id>
coral diff <capability-id> --upstream
coral update <capability-id> --check
coral update <capability-id>
coral check --json
```

Interpretation:

- `list` is the fast inventory and drift view.
- `diff` shows local changes against Coral’s pristine baseline.
- `diff --upstream` compares a Git-sourced capability with its latest source.
- `update --check` previews reconciliation without writing.
- `update` accepts intentional local edits or merges upstream changes,
  depending on the source.
- `check --json` is the CI gate.

### 4. Remove safely

```sh frame="terminal"
# Remove generated files, refusing modified files by default
coral delete <id> -a claude

# Remove generated files even when they changed
coral delete <id> -a claude --force

# Stop managing files but leave them in place
coral untrack <id> -a claude

# Remove an adapter registration only; capabilities remain untouched
coral agent remove claude
```

Use `delete` when Coral owns the generated files. Use `untrack` when the files
should remain available to the harness. `agent remove` is configuration-only;
it is not a cleanup command.

## Operations reference

### Safe defaults

- Start from the repository root or pass the harness directory flag.
- Commit shared instruction files and project capability definitions.
- Keep user state, credentials, local settings, and session transcripts out of
  the repository.
- Use plan / approval-on-request / workspace-write modes for exploratory work.
- Use JSON output in automation and preserve the process exit code.
- Run `coral check --json` before release or deployment automation.
- Review generated paths with `coral status` after adding or changing an
  adapter.

### Project versus global scope

Project scope is for reproducible team behavior. Global scope is for personal
defaults or organization-wide local setup. Coral follows the same distinction:

```sh frame="terminal"
coral init                 # project state
coral init --global        # user/global state
coral add skill ./pack     # project capability
coral add skill ./pack --global
coral agent set-default claude
coral agent set-default claude --global
```

When project and global capabilities share an id, the project copy wins. Use
`coral status` to find shadowing and override warnings.

For the Coral command and flag reference, see [CLI Reference](/cli). For concepts and
side effects, see [Lifecycle & Drift Detection](/concepts/lifecycle),
[Diffing & Updates](/concepts/diff-update), and [Scopes & Overrides](/concepts/scopes).

## Official references

- [Claude Code CLI reference](https://code.claude.com/docs/en/cli-usage),
  [settings](https://code.claude.com/docs/en/settings), and
  [.claude directory](https://code.claude.com/docs/en/claude-directory)
- [Codex CLI command reference](https://developers.openai.com/codex/cli/reference/)
  and [AGENTS.md discovery](https://developers.openai.com/codex/guides/agents-md/)
- [Cursor CLI usage](https://docs.cursor.com/en/cli/using),
  [parameters](https://docs.cursor.com/en/cli/reference/parameters), and
  [project rules](https://docs.cursor.com/context/rules)
- [OpenCode CLI](https://opencode.ai/docs/cli/),
  [configuration](https://opencode.ai/docs/config/), and
  [skills](https://opencode.ai/docs/skills)
