---
name: tuff-maintainer-guide
version: 1.0.0
description: Maintain Tuff's lockfile schema, capability lifecycle, and repository compatibility contracts.
triggers:
  - "bump lockfile version"
  - "lockfile schema"
  - "lockfile migration"
  - "maintain tuff"
  - "tuff repository maintenance"
allowed-tools: [Read, Write, Bash]
---

# Tuff Maintainer Guide

Use this skill when changing Tuff itself, especially its lockfile, manifest,
capability lifecycle, adapters, or repository maintenance workflows.

## Version vocabulary

Keep these versions separate:

- The top-level `version` in `.tuff/tuff-lock.json` is the lockfile schema
  version. It describes the JSON structure and interpretation rules.
- `version` in a capability's `tuff.toml` is the capability release version.
- The version in `Cargo.toml` is the Tuff CLI release version.
- A git-sourced capability can use its commit SHA as `installedVersion` because
  the commit identifies the installed source.

The maintainer skill's own `version` is only the skill release version. It does
not change the lockfile schema.

## When to bump the lockfile schema

Bump the lockfile schema only when an older reader cannot safely interpret the
new file. Examples include:

- removing or renaming required fields without a reader alias
- changing a field's type or meaning
- changing how targets, baselines, sources, or hashes are represented
- adding required data that cannot be inferred from an older file
- changing resolution behavior that requires a different persisted model

Do not bump the schema for:

- optional fields with safe defaults
- new capability types that fit the existing entry model
- terminology changes with compatible aliases
- normal capability releases
- Tuff CLI releases

## Current schema policy

Tuff is pre-release. The canonical lockfile schema is version `1`, with
`capabilities` and `type` as the persisted field names. Do not reintroduce
legacy `primitives` aliases unless a public compatibility promise requires it.

The lockfile reader intentionally rejects unsupported schema versions. This
makes format mistakes visible instead of silently misreading project state.

## Schema change checklist

Before changing the lockfile schema:

1. Decide whether the change is genuinely incompatible.
2. Update the schema constant, reader, writer, fixtures, and documentation.
3. Choose a compatibility policy: dual-version reading, explicit migration, or
   a documented pre-release reset.
4. Test old and new files, including missing fields and malformed values.
5. Test `tuff init`, creation, add, list, diff, check, and update flows.
6. Run formatting, linting, Rust tests, and the documentation build.
7. Document the minimum Tuff version required by repositories using the new
   schema.

## Baselines and capability versions

Changing a capability's `tuff.toml` version does not migrate the lockfile.
Tuff tracks emitted file hashes and baseline copies separately. `tuff update`
is source-aware: for in-place local entries it records intentional edits as the
new baseline; for external local and git-backed entries it refreshes from the
recorded source.

## Non-goals

- Do not treat every capability content change as a lockfile schema change.
- Do not silently rewrite a user's lockfile during an unrelated command.
- Do not remove compatibility code from a released format without a migration
  plan.
