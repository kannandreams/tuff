---
title: CLI Reference
description: Command reference for the Coral CLI.
---

Run commands from the repository root.

## `coral`

Show the ASCII banner and starter menu:

```sh
coral
```

This is the friendly entry point for engineers trying the CLI for the first
time. It does not mutate the repo.

## `coral init`

Initialize Coral state:

```sh
coral init
```

Creates `.coral/lock.json` if it does not already exist.

Initial contents:

```json
{
  "version": 1,
  "primitives": {}
}
```

`coral init` does not install capabilities, create skills, or apply defaults.
It only prepares the repository for lifecycle tracking. Commands that install
or inspect capabilities use this lockfile as their source of Coral state.

## `coral add <path>`

Install a local capability:

```sh
coral add examples/fixtures/python-uv-default
```

This example uses a demo fixture from the Coral repo. Production capabilities
should normally come from a user project directory or an external pack repo.

For the current Codex target, this installs:

```text
.agents/skills/<id>/SKILL.md
```

Coral refuses to overwrite an existing untracked skill at the same path.

## `coral list`

List installed capabilities:

```sh
coral list
```

Statuses:

- `clean`: installed content matches the recorded installed hash
- `modified`: installed content has changed locally
- `missing`: installed target file no longer exists

## `coral diff <id>`

Show a unified diff between the recorded baseline and the installed artifact:

```sh
coral diff python-uv-default
```

If there are no local changes, the command exits successfully with no diff
output.
