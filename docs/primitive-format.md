# Primitive Format

A primitive is a directory with a `loadout.toml` manifest and source files.

The current MVP supports only Codex skills.

The examples below use `python-uv-default` as a fixture. It demonstrates the
format; it is not a built-in default that Loadout core applies automatically.

## Directory layout

```text
python-uv-default/
  loadout.toml
  src/
    SKILL.md
```

## Manifest

```toml
id = "python-uv-default"
version = "0.1.0"
kind = "skill"
target = "codex"
description = "Use uv for Python dependency and environment management."
files = ["src/SKILL.md"]
```

## Fields

`id`
: Stable primitive identifier. For Codex skills, this becomes the skill
  directory name under `.agents/skills/`.

`version`
: Primitive version recorded in `.loadout/lock.json`.

`kind`
: Must be `skill` in the current implementation.

`target`
: Must be `codex` in the current implementation.

`description`
: Human-readable primitive summary.

`files`
: Must currently be `["src/SKILL.md"]`.

## Installed output

For `id = "python-uv-default"`, Loadout installs:

```text
.agents/skills/python-uv-default/SKILL.md
```

It also stores the install-time baseline at:

```text
.loadout/baselines/python-uv-default/SKILL.md
```

## Where primitives should live

Loadout core should stay content-agnostic. Production primitives should live in
one of two places:

- the user project that owns the primitive
- an external pack repository maintained by a person, team, or company

Fixtures under `examples/fixtures` are for tests and documentation examples.
