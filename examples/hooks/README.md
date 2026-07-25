# Tuff hook examples

These examples show the two hook source shapes supported by Tuff:

- `claude-session-start/` is a harness-native Claude hook. It contains a
  `settings.json` hook fragment and the script referenced by that fragment.
- A manifest-style hook can contain `tuff.toml` with a `[hook]` section. Use
  this shape for simple command hooks such as Open Agents hooks.

## Add the Claude example

From the repository root, run:

```sh
tuff add hook ./examples/hooks/claude-session-start \
  --agent claude \
  --hook-file settings.json
```

Tuff copies the script to `.claude/hooks/claude-session-start/` and merges
the hook registration into `.claude/settings.json`. It does not execute the
script during installation.

## Develop a hook inside the harness folder

If the source already lives under `.claude/`, Tuff adopts the files in place
instead of copying them:

```sh
tuff add hook .claude/hooks/session-start \
  --agent claude \
  --hook-file settings.json
```

The lockfile records this as an imported target. Tuff will not delete an
imported runtime file during `tuff remove`; use `tuff untrack` when you want
to stop managing it without removing the file.
