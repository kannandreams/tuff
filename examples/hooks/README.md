# Coral hook examples

These examples show the two hook source shapes supported by Coral:

- `claude-session-start/` is a harness-native Claude hook. It contains a
  `settings.json` hook fragment and the script referenced by that fragment.
- A manifest-style hook can contain `coral.toml` with a `[hook]` section. Use
  this shape for simple command hooks such as Open Agents hooks.

## Add the Claude example

From the repository root, run:

```sh
coral add hook ./examples/hooks/claude-session-start \
  --agent claude \
  --hook-file settings.json
```

Coral copies the script to `.claude/hooks/claude-session-start/` and merges
the hook registration into `.claude/settings.json`. It does not execute the
script during installation.

## Develop a hook inside the harness folder

If the source already lives under `.claude/`, Coral adopts the files in place
instead of copying them:

```sh
coral add hook .claude/hooks/session-start \
  --agent claude \
  --hook-file settings.json
```

The lockfile records this as an imported target. Coral will not delete an
imported runtime file during `coral remove`; use `coral untrack` when you want
to stop managing it without removing the file.
