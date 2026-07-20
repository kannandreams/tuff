---
title: Harness Adapters
description: How Coral maps one capability model into agent-specific files and config.
---

A harness adapter is the agent-specific layer that turns one Coral capability into the files a
particular coding environment expects.

The adapter decides where those files are emitted, what hook format is required, and where tool
registration lives.

## Supported Adapters

Coral currently ships two adapters:

| Adapter | Target id | Main output root |
|---|---|---|
| Open Agents | `open-agents` | `.agents/` |
| Claude | `claude` | `.claude/` |

`open-agents` is the shared `.agents/` layout Coral uses for agent-facing tools that can consume
the same structure. The current implementation lists support for Codex, Cursor, OpenCode, GitHub
Copilot, Gemini CLI, Roo, Cline, and Windsurf.

Legacy aliases are also accepted:

- `codex` resolves to `open-agents`
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

Coral writes the tool files and also registers the tool in the agent's MCP config so the harness
can discover it.

## Hooks

Hooks are where the adapter differences are most visible:

| Agent | Emitted path | Format |
|---|---|---|
| `open-agents` | `.agents/hooks/<id>/hook.toml` | TOML |
| `claude` | `.claude/hooks/<id>/hook.json` | JSON |

## Workflows

| Target | Emitted path |
|---|---|
| `open-agents` | `.agents/workflows/<id>/workflow.toml` |
| `claude` | `.claude/workflows/<id>/workflow.toml` |

## Supported capability types

Both adapters currently support:

- `skill`
- `tool`
- `hook`
- `workflow`

## Supported hook events

Hook event support is adapter-specific.

| Event | `open-agents` | `claude` |
|---|---|---|
| `before_finish` | Yes | Yes |
| `after_save` | Yes | No |
| `pre_tool_execution` | Yes | No |
| `post_tool_execution` | Yes | Yes |

If you try to install a hook with an unsupported event, Coral blocks the install and shows which
events that adapter accepts.

## Commands

```sh frame="terminal"
coral agent list
coral agent add open-agents
coral agent add claude
```
