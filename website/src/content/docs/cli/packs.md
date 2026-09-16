---
title: Packs
description: Build, publish, install, and update capability packs.
---

New to packs? Start with the [Tuff Pack examples repository](https://github.com/kannandreams/tuff-pack-examples). It follows tracked capabilities through build, inspection, GHCR publication, pull, extraction, and a container image before introducing the lower-level details. See [Capability Packs](/concepts/packs/) for the source manifest and artifact guarantees.

## `tuff pack`

### Build

Package all project-scoped tracked capabilities except `tuff-cli-guide`, using version `0.1.0` and the configured default agent:

```sh frame="terminal"
tuff pack build --name crm-integration
```

The default output is `tuff-dist/crm-integration-0.1.0.tuffpack`. Select capabilities, targets, a version, or a different output when needed:

```sh frame="terminal"
tuff pack build --name crm-integration --version 1.2.0 \
  --capability crm-operating --capability lead-triage \
  -a open-agents -a claude \
  --output releases/crm-integration-1.2.0.tuffpack
```

Create a reusable project-backed definition under `tuff-packs/<name>/tuff-pack.toml` without copying capability files:

```sh frame="terminal"
tuff pack init crm-integration --from-project \
  --capability crm-operating --capability lead-triage
tuff pack check tuff-packs/crm-integration
tuff pack build tuff-packs/crm-integration
```

Project builds fail when selected capability files or sources differ from the accepted `tuff.lock` baseline; accept intentional changes with `tuff update <capability>` first.

Standalone path-based source packs remain supported. Both commands default to the current directory:

```sh frame="terminal"
tuff pack check [path]
tuff pack build [path] --output <artifact.tuffpack>
```

For a standalone pack, omitting `--output` writes `<pack-name>-<pack-version>.tuffpack` beneath the pack root. Build always refuses to overwrite an existing artifact.

### Inspect and verify

```sh frame="terminal"
tuff pack inspect <artifact.tuffpack>
tuff pack inspect <artifact.tuffpack> --json
tuff pack verify <artifact.tuffpack>
```

### Push and pull

Publish a verified artifact to an OCI registry, or pull it back by an explicit tag or digest:

```sh frame="terminal"
tuff pack push <artifact.tuffpack> ghcr.io/yourorg/crm-integration:1.2.0
tuff pack pull ghcr.io/yourorg/crm-integration:1.2.0 --output crm-integration-1.2.0.tuffpack
tuff pack pull ghcr.io/yourorg/crm-integration@sha256:<manifest-digest> --output pinned.tuffpack
```

- `pack push` refuses to move a tag that already names different content unless `--force` is supplied. Pushing identical content is idempotent.
- `pack pull` resolves a tag to an immutable manifest digest before downloading, verifies the OCI descriptors and the complete Tuff artifact, and refuses to overwrite an existing output file.
- Both commands accept `--json`, repeatable `--ca-file <pem>`, and `--plain-http` for disposable development registries.
- Tuff uses existing Docker credentials first, then Podman credentials, and otherwise attempts anonymous access.

### Extract

Extract one pre-rendered adapter target without creating project lockfile state:

```sh frame="terminal"
tuff pack extract <artifact.tuffpack> -a <id> --output <directory>
```

The output directory must be missing or empty.

## Install a pack

Install every member of a verified local pack artifact into project scope:

```sh frame="terminal"
tuff init
tuff add pack ./tuff-dist/crm-integration-1.2.0.tuffpack -a open-agents
```

Pack installation verifies the complete artifact, preflights every member, stages shared hook and MCP configuration, and refuses the entire installation if any member is already tracked or would overwrite an untracked file. `--harness` is optional and repeatable; pack installation does not support `--global`.

If the pack came from a registry, pass `--reference` with the reference you
pulled it from so `tuff outdated` can check for a newer version later:

```sh frame="terminal"
tuff pack pull ghcr.io/acme/engineering:1.2.0 --output ./engineering.tuffpack
tuff add pack ./engineering.tuffpack -a open-agents \
  --reference ghcr.io/acme/engineering:1.2.0
```

`tuff add pack` only ever sees the local artifact file; it has no way to know
where it came from unless told. Without `--reference`, `tuff outdated` reports
this pack's capabilities as `not checked` rather than guessing.

## Update a pack

A capability installed by `tuff add pack` moves forward with its pack, never on its own. Naming any member updates every member:

- capabilities the new release drops are removed,
- new ones are installed,
- the rest are replaced, with shared hook and MCP registrations adjusted to match.

The pack is the unit of versioning and verification, so a lockfile never records two releases of one pack.

```sh frame="terminal"
# Resolve the registry recorded by `tuff add pack --reference`, pull the
# newest semver tag, and apply it
tuff update <member-id>

# Preview: the version, what would be added, updated, or removed, and
# whether local edits stand in the way
tuff update <member-id> --check

# Apply a pulled artifact instead of resolving the registry (offline, or a
# pack installed without --reference)
tuff update <member-id> --pack ./engineering-1.2.0.tuffpack

# Development registries, as for `tuff outdated`
tuff update <member-id> --plain-http --ca-file ./registry-ca.pem
```

A pack update applies to every agent the pack is installed for; a narrower `--harness` selection is refused rather than leaving one agent on the old release. Local edits to any member block the update unless `--force` is given. Only semver tags are compared when resolving the registry, matching `tuff outdated`; when nothing parses, pass `--pack` with the artifact you mean.
