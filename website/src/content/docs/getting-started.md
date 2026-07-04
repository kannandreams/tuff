---
title: Getting Started
description: Initialize Coral state and install your first capability.
---

This guide installs the `python-uv-default` sample fixture capability into a
repository. The fixture demonstrates the engine lifecycle; it is not a bundled
standard pack or a core product opinion.

## 1. Initialize Coral state

From the repository root:

```sh
coral init
```

This creates:

```text
.coral/lock.json
```

Initial contents:

```json
{
  "version": 1,
  "primitives": {}
}
```

`coral init` does not install anything. It only prepares the repository so
Coral can record installed capabilities later.

## 2. Add a capability

Install the sample fixture capability:

```sh
coral add examples/fixtures/python-uv-default
```

This writes:

```text
.agents/skills/python-uv-default/SKILL.md
.coral/baselines/python-uv-default/SKILL.md
```

The lockfile records the capability version, source path, installed target path,
baseline hash, and installed hash.

## 3. List installed capabilities

```sh
coral list
```

Expected output after a fresh install:

```text
python-uv-default	0.1.0	clean	.agents/skills/python-uv-default/SKILL.md
```

## 4. Edit the installed skill

Edit the installed file:

```text
.agents/skills/python-uv-default/SKILL.md
```

Coral treats this as normal repository ownership, not an error.

## 5. Check drift

```sh
coral list
```

The capability now reports `modified`.

## 6. Show the baseline diff

```sh
coral diff python-uv-default
```

The diff compares the installed skill against the baseline captured at install
time.

## Production use

For real projects, put capabilities in a project-owned directory or in a
separate pack repository. Coral core should provide the lifecycle mechanics;
your repo or pack should provide the content.
