# Primitive Format

A primitive is a directory with a `loadout.toml` manifest and source files.

The current MVP supports only Codex skills.

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
