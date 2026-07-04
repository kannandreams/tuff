---
title: Primitive Format
description: Manifest and file layout for Coral capabilities.
---

A Coral capability is a directory with a `coral.toml` manifest and source
files. The internal schema uses the term `primitive` because Coral will manage
several artifact kinds through one lifecycle engine.

The current MVP supports only Codex skills.

The examples below use `python-uv-default` as a fixture. It demonstrates the
format; it is not a built-in default that Coral core applies automatically.

## Directory layout

```text
python-uv-default/
  coral.toml
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
: Stable capability identifier. For Codex skills, this becomes the skill
  directory name under `.agents/skills/`.

`version`
: Capability version recorded in `.coral/lock.json`.

`kind`
: Must be `skill` in the current implementation.

`target`
: Must be `codex` in the current implementation.

`description`
: Human-readable capability summary.

`files`
: Must currently be `["src/SKILL.md"]`.

## Installed output

For `id = "python-uv-default"`, Coral installs:

```text
.agents/skills/python-uv-default/SKILL.md
```

It also stores the install-time baseline at:

```text
.coral/baselines/python-uv-default/SKILL.md
```

## Where capabilities should live

Coral core should stay content-agnostic. Production capabilities should live in
one of two places:

- the user project that owns the capability
- an external pack repository maintained by a person, team, or company

Fixtures under `examples/fixtures` are for tests and documentation examples.
