# Repository Model

Loadout core is the engine. It is not the place where engineers contribute
company-specific skills, hooks, tools, or workflows.

The useful model is three separate repository types:

| Layer | Analogy | Who touches it | What lives there |
| --- | --- | --- | --- |
| Loadout core | `dbt-core` | Contributors improving the engine | CLI, manifest schema, lockfile logic, validation, diff/merge, adapters |
| Loadout packs | `dbt-utils` or starter packages | Pack curators, teams, companies | Optional shared primitives, curated defaults, private company conventions |
| User projects | dbt projects | Engineers using Loadout day to day | Installed `.loadout/` state and locally modified primitives |

## Core is content-agnostic

Loadout core should not encode whether a team prefers `uv`, `pnpm`, a specific
commit convention, or a particular testing workflow. Those are content
decisions. Core provides the mechanics:

- parse primitive manifests
- install artifacts into a repo
- record baselines and lockfile state
- detect local drift
- show diffs
- validate primitive structure
- eventually merge upstream changes and compile to harness targets

If a company wants different primitives, it should not fork Loadout core. It
should use its own pack or keep primitives directly in its product repositories.

## Packs are optional content

A pack is a separate repo or directory containing primitives. A future
`loadout-pack-standard` could provide curated starter content, but it should be
installed through the same mechanism as any other pack.

That keeps the boundary clean:

- users can ignore the standard pack
- companies can maintain private packs
- teams can fork a pack without forking the engine
- core improvements still flow through normal Loadout releases

## User projects own installed primitives

Once a primitive is installed into a project, it belongs to that project. Local
edits are expected. Loadout should make those edits visible and manageable, not
forbid them.

The long-term lifecycle should look like this:

1. A project installs one or more primitives from a local source or pack.
2. Loadout records the baseline.
3. Engineers customize installed artifacts in their repo.
4. Loadout reports drift and validates structure.
5. When upstream changes arrive, Loadout shows a baseline/local/upstream merge.

Most engineers will use Loadout this way without ever contributing to the core
repo.

## What belongs in this repo

This repo should contain:

- the CLI engine
- primitive schema and validation
- lockfile and baseline mechanics
- diff and future merge logic
- harness adapters
- tests and demo fixtures

This repo should not contain production primitive opinions as core behavior.
The `examples/fixtures` primitives exist to test and demonstrate the engine.
