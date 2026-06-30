# Loadout

Loadout is a small CLI for managing repo-local agent primitives.

The first milestone focuses on one practical loop:

1. Install a local primitive into a repository.
2. Record the installed baseline.
3. Let the repository owner edit the installed artifact.
4. Detect that local drift.
5. Show a diff against the recorded baseline.

The initial target is Codex-style skills installed at
`.agents/skills/<id>/SKILL.md`.

## Why Loadout exists

Agent coding tools increasingly depend on project-local conventions: skills,
rules, hooks, workflows, and setup notes. Those conventions often get copied
between repositories by hand, then drift over time.

Loadout treats installed primitives as repo-owned files while still keeping a
baseline that makes local changes visible.

## Current scope

Loadout currently supports:

- local filesystem primitives
- `kind = "skill"`
- `target = "codex"`
- install, list, drift detection, and baseline diff

Loadout does not yet support remote registries, multiple harness targets,
hooks, tools, workflows, or update merges.

## Positioning

Loadout is not trying to be another broad skill marketplace. Vercel Skills has
already validated that category for skill installation and discovery across
many agents.

Loadout's narrower bet is typed lifecycle management for repo-local primitives:
skills first, then tools, hooks, and workflows, with stronger drift, diff, and
future merge behavior for teams that intentionally customize installed
artifacts.

See the [Vercel Skills comparison](comparison/vercel-skills.md) for the
strategic distinction.
