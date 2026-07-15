---
title: Hooks
description: Hooks define event-driven automation that runs at specific lifecycle moments.
---

A hook capability represents automation that runs at defined moments in an agent lifecycle.
Hooks are useful for validation, formatting, enforcement, and project-specific checks.

Because hooks run automatically (triggered by the harness, never by Coral), Coral applies the
same safety rules as tools: no execution during install, path traversal rejection, and clear
reporting of what the hook does.

## Manifest

```toml
# coral.toml
id = "pre-commit-lint"
version = "1.0.0"
primitive = "hook"
description = "Run lint checks before the agent finishes a session."

[hook]
event = "before_finish"
command = "cargo fmt --check && cargo clippy -- -D warnings"
working_directory = "."
```

| Field | Required | Description |
|---|---|---|
| `id` | Yes | Stable identifier |
| `version` | Yes | Semantic version |
| `primitive` | Yes | Must be `"hook"` |
| `description` | Yes | What the hook does and when it fires |
| `hook.event` | Yes | Adapter-validated event name |
| `hook.command` | Yes | Shell command to execute |
| `hook.working_directory` | No | Working directory (default: `.`), path traversal rejected |

## Supported events

Events are validated by the target adapter at install time. Each adapter maintains a
list of supported event names.

### Open Agents (`open-agents`)

| Event | Description |
|---|---|
| `before_finish` | Before the agent completes a session or task |
| `after_save` | After a file has been saved |
| `pre_tool_execution` | Before a tool call is executed |
| `post_tool_execution` | After a tool call completes |

### Claude (`claude`)

| Event | Description |
|---|---|
| `before_finish` | Before the agent completes a response |
| `post_tool_execution` | After a tool finishes executing |

## Installing a hook

```sh frame="terminal"
# Local directory
coral add ./pre-commit-lint -t open-agents

# Git repository
coral add https://github.com/owner/repo --hook pre-commit-lint -t open-agents

# Multiple targets
coral add ./pre-commit-lint -t open-agents -t claude
```

Install-time validation:

```sh frame="terminal"
$ coral add ./pre-commit-lint -t open-agents
note: this hook runs 'cargo fmt --check && cargo clippy -- -D warnings'
      on event 'before_finish': it will not be executed during install
installed pre-commit-lint (open-agents) -> .agents/hooks/pre-commit-lint/hook.toml
```

## Where files go

| Target | Hook directory | Format |
|---|---|---|
| `open-agents` | `.agents/hooks/<id>/hook.toml` | TOML |
| `claude` | `.claude/hooks/<id>/hook.json` | JSON |

### Emitted hooks

```toml
# .agents/hooks/pre-commit-lint/hook.toml
event = "before_finish"
command = "cargo fmt --check && cargo clippy -- -D warnings"
working_directory = "."
```

```json
// .claude/hooks/pre-commit-lint/hook.json
{
  "event": "before_finish",
  "command": "cargo fmt --check && cargo clippy -- -D warnings",
  "working_directory": "."
}
```

## Filtering

```sh frame="terminal"
coral list --type hook
```

## Lifecycle

Hooks participate in the full Coral lifecycle: baseline capture, drift detection, diff,
and merge-aware updates just like skills and tools.

```sh frame="terminal"
$ coral diff pre-commit-lint
--- baseline/open-agents/pre-commit-lint/
+++ .agents/hooks/pre-commit-lint/hook.toml
-event = "before_finish"
+event = "after_save"
```

## Safety

:::caution
Hooks run automatically when triggered by the harness. They are never executed by
Coral during install or update. Review `hook.command` carefully. A hook that
modifies files or runs destructive commands affects every agent session.
:::
