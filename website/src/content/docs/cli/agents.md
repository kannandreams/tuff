---
title: Harnesses and Scope
description: Register harnesses, set the default harness, and choose between project and global scope.
---

## `tuff harness`

A harness is the coding agent product Tuff installs for, such as Claude Code or Codex. Its id, such as `claude`, is what `-a` selects.

:::note[Older names]
Tuff 0.11 and earlier call this command `tuff agent` and the flag `--agent`. Later versions accept only the new names. The short flag `-a` works in every version.
:::

### Configure the default harness

```sh frame="terminal"
# Project default
tuff harness set-default open-agents

# Global default
tuff harness set-default claude --global
```

Commands that accept `-a/--harness` use this value when the flag is omitted.
An explicit `-a` always overrides the default, and repeated flags apply an
operation to several harnesses.

### List available and registered harnesses

```sh frame="terminal"
tuff harness list

# Show the global default
tuff harness list --global
```

The `REGISTERED` column shows which harnesses are available for Tuff operations in
the selected config. The `DEFAULT` column shows which registered harness is used
when `-a/--harness` is omitted.

### Register a harness

```sh frame="terminal"
tuff harness add open-agents
tuff harness add claude
tuff harness add codex
tuff harness add cursor
tuff harness add opencode
```

Registering a harness also creates its project directory (`.agents/` or
`.claude/`) if it does not already exist.

`claude-code` remains an alias for `claude`. `codex`, `cursor`, and `opencode` are dedicated adapter IDs. `opencode` takes policies and MCP servers, and `tuff init` does not register it.

### Remove a harness

```sh frame="terminal"
tuff harness remove open-agents
```

Unregisters the harness from the project configuration. It does not delete
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
