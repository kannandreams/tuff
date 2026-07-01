# Loadout

Loadout is a small CLI for managing repo-local agent primitives. It is the
engine for primitive lifecycle management, not a repository of company-specific
skills.

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

Loadout core is content-agnostic. Opinionated primitives belong in separate
pack repos or in the user projects that own them. See the
[Repository Model](repository-model.md) for the `dbt-core` style split between
core, packs, and user projects.

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

Optional starter packs can exist later, but they should sit above Loadout core
instead of becoming core behavior.

See the [Vercel Skills comparison](comparison/vercel-skills.md) for the
strategic distinction.
