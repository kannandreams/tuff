---
title: Stability
description: What the Tuff 1.0 compatibility promise covers, what it excludes, and how file formats change.
---

Tuff is version 0.x today. In 0.x, any minor release can change commands, flags, output, and file formats, and the [changelog](/changelog) lists each change. The promise on this page applies from 1.0.0. From then on, a change that breaks anything listed under [What 1.0 covers](#what-10-covers) needs a major version.

## What 1.0 covers

- **CLI commands, flags, and exit codes.** The commands and flags in the [CLI reference](/cli), and the [exit codes](/cli/#exit-codes-and-errors) `0`, `1`, `2`, and `70`.
- **`--json` output.** The shape of the output of every command that takes `--json`, including the error object written to stderr and its `kind` values.
- **The `tuff.toml` manifest.** The fields and values described on [The tuff.toml File](/primitives/format).
- **The `tuff.lock` format.** Schema version 3, described on [The tuff.lock File](/concepts/lockfile), and the migration rules below.
- **The hooks specification.** The [Hooks Specification](/spec/hooks), including its event vocabulary and the files an implementation writes. The specification keeps its own version number and the rules in its section 9. Its compatibility matrices describe what each harness supports, and they can change when a harness changes, without a new specification version.
- **File locations in each harness.** The paths Tuff writes into each harness's folder, listed on [Harness Adapters](/concepts/adapters).

## What 1.0 excludes

These can change in any 1.x release.

- **Policies.** Policy capabilities, `tuff policy matrix`, and the `--accept-unenforced` flag are [Preview](/primitives/policies).
- **The dashboard.** `tuff dashboard publish`, the report it builds, and the dashboard server are [in progress](/cli/dashboard).
- **The Rust API of the crates.** Tuff publishes its crates, such as `tuff-core`, to crates.io with the same version number as the CLI. The promise covers the CLI and the file formats. The Rust API of the crates can change in any minor release.

A feature that is added after 1.0 and labelled Preview is excluded until its page stops carrying that label.

## How file formats change

`tuff.lock` and the hooks specification carry a version. `tuff.toml` has no format version.

### tuff.lock today

- Tuff reads schema versions 1, 2, and 3. Versions 1 and 2 are TOML and version 3 is JSON.
- Every command that changes the lockfile writes version 3. Commands that only read it leave an older file as it is.
- `tuff lock migrate` rewrites the file as version 3 and changes nothing else.
- A lockfile with a version newer than the running Tuff understands is refused with an `unsupported` error that names the file's version, the versions the running Tuff reads, and the instruction to upgrade Tuff. The error exits with code `1`.
- A file whose syntax does not match its version, such as TOML that claims version 3, is refused as `corrupt`.

### A new format version after 1.0

- Tuff reads the new version and every older version it reads today.
- Tuff writes the new version when a command changes the file, and `tuff lock migrate` does the upgrade on its own.
- A Tuff release from 1.0 on that predates the new version refuses the file with the same `unsupported` error, so a teammate on that release gets an instruction to upgrade.
- If a release stops reading an older version, the changelog says so first.

### tuff.toml

The `id`, `version`, `type`, and `description` fields, and the sections for each type, are the manifest format. The manifest's `version` is the version of the capability. A manifest carries no version for its own format. Tuff ignores top-level keys it does not know, and it refuses a `type` it does not know.

### Hooks specification

The specification follows its own rules: a patch rewords, a minor adds, and a major removes or renames. Each Tuff release implements exactly one specification version, and the [compatibility statement](/spec/hooks#9-versioning) lists which.
