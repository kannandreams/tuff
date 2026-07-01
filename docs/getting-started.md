# Getting Started

This guide installs the `python-uv-default` sample fixture primitive into a
repository. The fixture demonstrates the engine lifecycle; it is not a bundled
standard pack or a core product opinion.

## 1. Initialize Loadout state

From the repository root:

```sh
uv run loadout init
```

This creates:

```text
.loadout/lock.json
```

## 2. Add a primitive

Install the sample fixture primitive:

```sh
uv run loadout add examples/fixtures/python-uv-default
```

This writes:

```text
.agents/skills/python-uv-default/SKILL.md
.loadout/baselines/python-uv-default/SKILL.md
```

The lockfile records the primitive version, source path, installed target path,
baseline hash, and installed hash.

## 3. List installed primitives

```sh
uv run loadout list
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

Loadout treats this as normal repository ownership, not an error.

## 5. Check drift

```sh
uv run loadout list
```

The primitive now reports `modified`.

## 6. Show the baseline diff

```sh
uv run loadout diff python-uv-default
```

The diff compares the installed skill against the baseline captured at install
time.

## Production use

For real projects, put primitives in a project-owned directory or in a separate
pack repository. Loadout core should provide the lifecycle mechanics; your repo
or pack should provide the content.
