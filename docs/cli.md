# CLI Reference

Run commands from the repository root.

## `loadout init`

Initialize Loadout state:

```sh
uv run loadout init
```

Creates `.loadout/lock.json` if it does not already exist.

## `loadout add <primitive-path>`

Install a local primitive:

```sh
uv run loadout add examples/fixtures/python-uv-default
```

This example uses a demo fixture from the Loadout repo. Production primitives
should normally come from a user project directory or an external pack repo.

For the current Codex target, this installs:

```text
.agents/skills/<id>/SKILL.md
```

Loadout refuses to overwrite an existing untracked skill at the same path.

## `loadout list`

List installed primitives:

```sh
uv run loadout list
```

Statuses:

- `clean`: installed content matches the recorded installed hash
- `modified`: installed content has changed locally
- `missing`: installed target file no longer exists

## `loadout diff <id>`

Show a unified diff between the recorded baseline and the installed artifact:

```sh
uv run loadout diff python-uv-default
```

If there are no local changes, the command exits successfully with no diff
output.
