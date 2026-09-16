---
title: Agents and Scope
description: Register agent harnesses, set the default agent, and choose between project and global scope.
---

## `tuff agent`

### Configure the default agent

```sh frame="terminal"
# Project default
tuff agent set-default open-agents

# Global default
tuff agent set-default claude --global
```

Commands that accept `-a/--agent` use this value when the flag is omitted.
An explicit agent flag always overrides the default, and repeated flags still
apply an operation to multiple agents.

### List available and registered agents

```sh frame="terminal"
tuff agent list

# Show the global default
tuff agent list --global
```

The `REGISTERED` column shows which agents are available for Tuff operations in
the selected config. The `DEFAULT` column shows which registered agent is used
when `-a/--agent` is omitted.

### Register an agent

```sh frame="terminal"
tuff agent add open-agents
tuff agent add claude
tuff agent add codex
tuff agent add cursor
tuff agent add opencode
```

Registering an agent also creates its project directory (`.agents/` or
`.claude/`) if it does not already exist.

`claude-code` remains an alias for `claude`. `codex`, `cursor`, and `opencode` are dedicated adapter IDs. `opencode` takes policies and MCP servers, and `tuff init` does not register it.

### Remove an agent

```sh frame="terminal"
tuff agent remove open-agents
```

Unregisters the agent from the project configuration. It does not delete
capabilities, emitted files, baselines, MCP registrations, or lockfile entries.
Use [`tuff delete <id>`](/cli/clean-up/#tuff-delete) or [`tuff untrack <id>`](/cli/clean-up/#tuff-untrack) for the configured default
agent. Pass `-a <agent>` when selecting a different agent.

## Scope

Tuff supports two scopes:

| Scope | Location | Use |
|---|---|---|
| `project` | `tuff.lock` in repo root | Shared with team via version control |
| `global` | Tuff user state directory | Available across all projects |

Resolution order: **project always wins**. If the same primitive exists at both scopes,
the project copy shadows the global one. `tuff status` flags shadowed primitives.
See [Scopes & Overrides](/concepts/scopes) for more.
