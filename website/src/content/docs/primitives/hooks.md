---
title: Hooks
description: Hooks define event-driven automation that runs at specific lifecycle moments.
---

A hook capability represents automation that runs at defined moments in an agent lifecycle.
Hooks are useful for validation, formatting, enforcement, and project-specific checks.

Because hooks run automatically (triggered by the harness, never by Coral), Coral applies the
same safety rules as tools: no execution during install, path traversal rejection, and clear
reporting of what the hook does.

## Source shape

For harness-native hooks, keep the runtime files and a small hook fragment in one folder:

```text
claude-session-start/
  settings.json
  session-start.sh
  README.md
```

The hook fragment should contain only the native hook registration, not the user's full
harness settings file. For Claude, that means a `hooks`-only JSON fragment:

```json
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "sh {{hook_dir}}/session-start.sh"
          }
        ]
      }
    ]
  }
}
```

Coral replaces `{{hook_dir}}` with the managed hook directory for that harness.

## Installing a hook

To create a new tracked hook scaffold, use `coral create hook`. For Claude, Coral creates the
runtime command file and registers it in the shared settings file:

```sh frame="terminal"
coral create hook session-start --agent claude
```

This produces:

```text
.claude/
  settings.json
  hooks/
    session-start/
      run.sh
```

Edit `run.sh` and the generated `SessionStart` registration in `.claude/settings.json`. Coral
tracks both files in the lockfile. Open Agents follows the same shape:

```sh frame="terminal"
coral create hook review-hook --agent open-agents
```

This creates `.agents/hooks/review-hook/run.sh` and registers the `before_finish` event in
`.agents/hook.json`.

The repository includes a complete native Claude example at
`examples/hooks/claude-session-start/`. Its source directory contains the
hook fragment and the runtime script, but no `.claude/` target directory:

```text
examples/hooks/claude-session-start/
  settings.json       # hooks-only native fragment
  session-start.sh    # runtime file referenced by the fragment
```

Install that example with:

```sh frame="terminal"
coral add hook ./examples/hooks/claude-session-start \
  --agent claude \
  --hook-file settings.json
```

If the source lives outside the harness folder, Coral copies runtime files into the selected
harness and merges the hook fragment into the native settings file:

```text
.claude/
  settings.json
  hooks/
    claude-session-start/
      session-start.sh
```

If the source already lives under the selected harness folder, Coral adopts it in place instead
of copying it:

```sh frame="terminal"
coral add hook .claude/hooks/session-start --agent claude --hook-file settings.json
```

The in-place form is useful while developing a hook directly inside `.claude/`, `.cursor/`, or
another harness-specific folder.

The same source can also come from Git. The capability name is the directory inside the
repository:

```sh frame="terminal"
coral add hook https://github.com/acme/coral-hooks claude-session-start \
  --agent claude \
  --hook-file settings.json
```

For Git sources, Coral resolves `settings.json` relative to the named hook directory, copies
the runtime files into `.claude/hooks/<id>/`, merges the fragment, and records the Git revision
in `.coral/coral-lock.json`.

## Hook event reference

### Open Agents (`open-agents`)

| Event | Description |
|---|---|
| `before_finish` | Before the agent completes a session or task |
| `after_save` | After a file has been saved |
| `pre_tool_execution` | Before a tool call is executed |
| `post_tool_execution` | After a tool call completes |

### Claude (`claude`)

For native hook fragments, Coral reads the event names from the fragment and merges them into the
adapter's settings file.

```sh frame="terminal"
$ coral create hook review-hook --agent open-agents
created and tracked hook 'review-hook' (open-agents) -> .agents/hook.json
```

## Where files go

| Target | Hook directory | Format |
|---|---|---|
| `open-agents` | `.agents/hooks/<id>/run.sh` plus `.agents/hook.json` | Native JSON (development format) |
| `claude` | `.claude/hooks/<id>/...` plus `.claude/settings.json` | Native Claude JSON |

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
+++ .agents/hooks/pre-commit-lint/run.sh
-echo "replace with hook logic"
+cargo fmt --check
```

## Safety

:::caution
Hooks run automatically when triggered by the harness. They are never executed by
Coral during install or update. Review the command in the hook settings file carefully. A hook that
modifies files or runs destructive commands affects every agent session.
:::
