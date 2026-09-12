---
title: Hooks
description: Hooks define event-driven automation that runs at specific lifecycle moments.
---

A hook capability represents automation that runs at defined moments in an agent lifecycle.
Hooks are useful for validation, formatting, enforcement, and project-specific checks.

Tuff supports two hook source shapes:

- **Tuff-standard hooks** use a `tuff.toml` `[hook]` section with canonical event names.
  Tuff validates those events against the selected adapter and renders them into that
  harness's native hook settings.
- **Native harness hooks** pass a harness hook fragment with `--hook-file`. Tuff copies or
  adopts the runtime files and merges the native fragment as-is.

Because hooks run automatically (triggered by the harness, never by Tuff), Tuff applies the
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

Tuff replaces `{{hook_dir}}` with the managed hook directory for that harness.

## Installing a hook

To create a new tracked hook scaffold, use `tuff create hook`. For Claude, Tuff creates the
runtime command file and registers it in the shared settings file:

```sh frame="terminal"
tuff create hook session-start --agent claude
```

This produces:

```text
.claude/
  settings.json
  hooks/
    session-start/
      run.sh
```

Edit `run.sh` and the generated `SessionStart` registration in `.claude/settings.json`. Tuff
tracks both files in the lockfile. Open Agents follows the same shape:

```sh frame="terminal"
tuff create hook review-hook --agent open-agents
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
tuff add hook ./examples/hooks/claude-session-start \
  --agent claude \
  --hook-file settings.json
```

If the source lives outside the harness folder, Tuff copies runtime files into the selected
harness and merges the hook fragment into the native settings file:

```text
.claude/
  settings.json
  hooks/
    claude-session-start/
      session-start.sh
```

If the source already lives under the selected harness folder, Tuff adopts it in place instead
of copying it:

```sh frame="terminal"
tuff add hook .claude/hooks/session-start --agent claude --hook-file settings.json
```

The in-place form is useful while developing a hook directly inside `.claude/`, `.cursor/`, or
another harness-specific folder.

The same source can also come from Git. The capability name is the directory inside the
repository:

```sh frame="terminal"
tuff add hook https://github.com/yourorg/tuff-hooks claude-session-start \
  --agent claude \
  --hook-file settings.json
```

For Git sources, Tuff resolves `settings.json` relative to the named hook directory, copies
the runtime files into `.claude/hooks/<id>/`, merges the fragment, and records the Git revision
in `tuff.lock`.

## Hook event reference

Manifest-style Tuff-standard hooks use canonical event names: `session_start`, `session_end`, `pre_tool_use`, `post_tool_use`, `before_finish`, `after_save`, and `stop`. Tuff maps aliases such as `pre_tool_execution` to the canonical `pre_tool_use` event where a harness supports it, and each harness declares how faithfully it can honour every event: fully, partially with a stated caveat, or not at all, in which case the install is refused with the list of events that harness does support.

The full vocabulary, what each event may block, and every harness's compatibility matrix are published in the [Hooks Specification](/spec/hooks/). Those tables are generated from the code Tuff runs, so they cannot drift from what `tuff add` does. The short version:

- **Open Agents** and **Codex** render into `.agents/hook.json` with events such as `before_finish`, `after_save`, `pre_tool_execution`, and `post_tool_execution`.
- **Claude** renders into `.claude/settings.json` with Claude Code's case-sensitive names: `SessionStart`, `SessionEnd`, `PreToolUse`, `PostToolUse`, and `Stop`. `before_finish` is partial through `Stop`, and `after_save` is unsupported because Claude's `FileChanged` needs watched paths the standard hook cannot express.
- **Cursor** renders into `.cursor/hooks.json` with `sessionStart`, `sessionEnd`, `preToolUse`, `postToolUse`, and `stop`, where a hook group carries a direct `command` field rather than the nested `hooks` array the others use.

Use the compatibility commands to inspect what Tuff-standard hook events can render where:

```sh frame="terminal"
tuff hooks matrix
tuff hooks check-portability pre-commit-lint --target claude
tuff hooks spec
```

`tuff hooks matrix` covers the agents registered in the project; `tuff hooks spec` prints the whole specification your binary implements, every harness included, and `--json` prints the document the published specification is generated from.

Portability checks are scoped to registered adapters. Tuff-standard hooks retain both their
canonical event and emitted native event in the lockfile, so the target adapter is checked using the
canonical event. Native hook fragments fall back to native-event matching and are not guaranteed to
be portable.

```sh frame="terminal"
$ tuff create hook review-hook --agent open-agents
created and tracked hook 'review-hook' (open-agents) -> .agents/hook.json
```

## Where files go

| Target | Hook directory | Format |
|---|---|---|
| `open-agents` | `.agents/hooks/<id>/run.sh` plus `.agents/hook.json` | Native JSON (development format) |
| `claude` | `.claude/hooks/<id>/...` plus `.claude/settings.json` | Native Claude JSON |
| `codex` | `.agents/hooks/<id>/run.sh` plus `.agents/hook.json` | Codex hook JSON |
| `cursor` | `.cursor/hooks/<id>/run.sh` plus `.cursor/hooks.json` | Cursor Hooks JSON |

## Filtering

```sh frame="terminal"
tuff list --type hook
```

## Lifecycle

Hooks participate in the full Tuff lifecycle: baseline capture, drift detection, diff,
and merge-aware updates just like skills and tools.

```sh frame="terminal"
$ tuff diff pre-commit-lint
--- baseline/open-agents/pre-commit-lint/
+++ .agents/hooks/pre-commit-lint/run.sh
-echo "replace with hook logic"
+cargo fmt --check
```

## Safety

:::caution
Hooks run automatically when triggered by the harness. They are never executed by
Tuff during install or update. Review the command in the hook settings file carefully. A hook that
modifies files or runs destructive commands affects every agent session.
:::
