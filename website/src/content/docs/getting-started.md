---
title: Getting Started
description: Initialize Coral state and install your first capability.
---

This guide installs the `python-uv-default` sample fixture into a project.
The fixture demonstrates the engine lifecycle; it is not a bundled standard pack.

## 1. Initialize Coral state

```sh
coral init
```

This creates:

```text
.coral/coral-lock.json
.coral/config.json
```

## 2. Install a capability

```sh
# Skill (local)
coral add examples/fixtures/python-uv-default -t open-agents

# Skill (git)
coral add https://github.com/owner/repo --skill python-uv-default -t open-agents

# Tool
coral add examples/fixtures/security-review -t claude

# Hook
coral add examples/fixtures/pre-commit-lint -t open-agents

# Global scope
coral add examples/fixtures/python-uv-default -t open-agents --global
```

This writes emitted files to the target harness directory and records baselines
under `.coral/baselines/`.

## 3. List installed capabilities

```sh
coral list
```

Expected output:

```
python-uv-default  0.1.0   project   open-agents   clean   .agents/skills/python-uv-default/SKILL.md
```

## 4. Check status

```sh
coral status
```

Shows per-primitive scope, drift, and override warnings.

## 5. Edit and detect drift

Edit the installed file:

```
.agents/skills/python-uv-default/SKILL.md
```

Then check:

```sh
coral list
```

The capability now reports `modified`.

## 6. Show the diff

```sh
coral diff python-uv-default
```

Compares the installed file against the baseline captured at install time.

## 7. Update from git source

```sh
coral update python-uv-default --check   # dry run
coral update python-uv-default            # merge or apply
```

## What to commit

Commit `.coral/` and the emitted agent files together:

```sh
git add .coral/coral-lock.json .coral/config.json .coral/baselines/
git add .agents/ .claude/
```

Your team then has the full lifecycle state without re-installing.
See the [lockfile reference](/concepts/lockfile) for the complete directory structure.
