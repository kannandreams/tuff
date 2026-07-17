---
title: coral.toml
description: Manifest and file layout for Coral capabilities.
---

Every Coral capability is described by a `coral.toml` manifest. The manifest
declares the capability type, the files that belong to it, and enough metadata
for Coral to track installs, validate structure, detect drift, and emit
agent-specific agent output.

`coral.toml` travels with the capability. It is not the project index and it is
not the lockfile. Coral keeps project tracking and baselines in
`.coral/coral-lock.json`, and can generate readable derived files such as
`.agents/CAPABILITIES.md` with `coral generate index`.

The `type` field is the important discriminator. It tells Coral whether the
directory should be treated as a skill, tool, hook, policy, or workflow while
still using the same lifecycle commands: install, list, check, diff, update,
and remove.

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
type = "skill"
description = "Use uv for Python dependency and environment management."
files = ["src/SKILL.md"]
```

## Fields

`id`
: Stable capability identifier. For skills, this becomes the skill directory
  name in the agent harness output.

`version`
: Capability version recorded in `.coral/coral-lock.json`.

`type`
: Capability type. Supported values include `skill`, `tool`, `hook`, and
  `workflow`. Policies are part of the capability model, but may not be
  available in every adapter yet.

`description`
: Human-readable capability summary.

`files`
: Source files managed as part of this capability.

## Installed output

For a skill with `id = "python-uv-default"`, Coral installs:

```text
.agents/skills/python-uv-default/SKILL.md
```

It also records an install-time baseline under `.coral/` so later edits can be
reported as drift:

```text
.coral/baselines/open-agents/python-uv-default/
```

## Where capabilities should live

Coral core should stay content-agnostic. Production capabilities should live in
one of two places:

- the user project that owns the capability
- an external pack repository maintained by a person, team, or company

Fixtures under `examples/fixtures` are for tests and documentation examples.
