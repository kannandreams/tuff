---
title: Harness Adapters
description: How Coral maps one capability model into agent-specific files and config.
---

A harness is the coding environment Coral emits into, such as `.agents/` or `.claude/`.
A harness adapter is Coral's agent-specific implementation layer for that environment.

The adapter decides where files are emitted, how native settings are updated, and where tool
registration lives.

## Supported Adapters

Coral currently ships four adapters:

| Adapter | Target id | Main output root |
|---|---|---|
| Open Agents | `open-agents` | `.agents/` |
| Claude | `claude` | `.claude/` |
| Codex | `codex` | `.agents/` |
| Cursor | `cursor` | `.cursor/` |

`open-agents` remains the generic shared `.agents/` adapter. Codex now has a dedicated adapter even
though it currently emits the same directory family, because its hook coverage and native behavior
are tracked independently.

Legacy aliases are also accepted:

- `claude-code` resolves to `claude`

The same capability can be emitted differently depending on the agent.

## Skills

| Agent | Emitted path |
|---|---|
| `open-agents` | `.agents/skills/<id>/...` |
| `claude` | `.claude/skills/<id>/...` |

## Tools

| Agent | Emitted path | MCP registration |
|---|---|---|
| `open-agents` | `.agents/tools/<id>/...` | `.agents/mcp.json` |
| `claude` | `.claude/tools/<id>/...` | `.mcp.json` |
| `codex` | `.agents/tools/<id>/...` | `.agents/mcp.json` |
| `cursor` | `.cursor/tools/<id>/...` | `.cursor/mcp.json` |

Coral writes the tool files and also registers the tool in the agent's MCP config so the harness
can discover it.

## Hooks

Hooks are where the adapter differences are most visible:

| Agent | Emitted path | Format |
|---|---|---|
| `open-agents` | `.agents/hooks/<id>/run.sh` plus `.agents/hook.json` | Native JSON (development format) |
| `claude` | `.claude/hooks/<id>/...` plus `.claude/settings.json` | Native Claude JSON |
| `codex` | `.agents/hooks/<id>/run.sh` plus `.agents/hook.json` | Codex hook JSON |
| `cursor` | `.cursor/hooks/<id>/run.sh` plus `.cursor/hooks.json` | Cursor Hooks JSON |

For Claude, `coral add hook ... --hook-file settings.json` reads a hooks-only native fragment,
copies runtime files when needed, and merges the fragment into `.claude/settings.json`.

## Workflows

| Target | Emitted path |
|---|---|
| `open-agents` | `.agents/workflows/<id>/workflow.toml` |
| `claude` | `.claude/workflows/<id>/workflow.toml` |
| `codex` | `.agents/workflows/<id>/workflow.toml` |
| `cursor` | `.cursor/workflows/<id>/workflow.toml` |

## Supported capability types

All four adapters currently support:

- `skill`
- `tool`
- `hook`
- `workflow`

## Supported hook events

Manifest-style Coral-standard hook event support is adapter-specific. Native hook fragments
supplied with `--hook-file` keep their harness event names and are merged as-is.

| Event | `open-agents` | `claude` | `codex` | `cursor` |
|---|---|---|---|---|
| `before_finish` | Yes | Yes | Yes | Partial, rendered as `stop` |
| `after_save` | Yes | No | Yes | No |
| `pre_tool_use` | Yes, rendered as `pre_tool_execution` | No | Partial, rendered as `pre_tool_execution` | Yes, rendered as `preToolUse` |
| `post_tool_use` | Yes, rendered as `post_tool_execution` | Yes, rendered as `post_tool_execution` | Partial, rendered as `post_tool_execution` | Yes, rendered as `postToolUse` |
| `session_start` | No | Yes, rendered as `SessionStart` | No | Yes, rendered as `sessionStart` |
| `session_end` | No | No | No | Yes, rendered as `sessionEnd` |
| `stop` | No | No | No | Yes, rendered as `stop` |

Codex and Cursor have dedicated compatibility rows even when their output roots overlap with the
generic Open Agents adapter. Cursor renders native names such as `sessionStart`, `preToolUse`,
`postToolUse`, and `stop` into `.cursor/hooks.json`.

If you try to install a manifest-style hook with an unsupported event, Coral blocks the install
and shows which events that adapter accepts. Run `coral hooks matrix` to inspect the registered
adapter compatibility matrix, or `coral hooks check-portability <id> --target <adapter>` to check
an installed hook before switching adapters.

## Commands

```sh frame="terminal"
coral agent list
coral agent add open-agents
coral agent add claude
coral agent add codex
coral agent add cursor
coral hooks matrix
```
