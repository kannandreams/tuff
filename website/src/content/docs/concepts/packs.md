---
title: Capability Packs
description: Build, verify, extract, and install immutable bundles of Tuff capabilities.
---

A Tuff pack is a versioned release unit containing one or more manifest-backed capabilities. Each member keeps its own capability ID, type, and release version; the pack adds a separate name and version for promoting the collection as one artifact.

Use packs when a platform team needs to deliver the same reviewed skills, tools, hooks, and workflows into multiple repositories or ephemeral agent runtimes. Continue using `tuff add <capability-type>` when you only need to manage one capability.

## Source layout

```text
company-agent-pack/
  tuff-pack.toml
  capabilities/
    rust-guidance/
      tuff.toml
      SKILL.md
    security-review/
      tuff.toml
      review.sh
    release-prep/
      tuff.toml
```

Every member must be a local directory beneath the pack root and must contain a valid `tuff.toml`. Remote members, symbolic links, inferred manifests, and paths that escape the pack root are rejected.

```toml title="tuff-pack.toml"
schema = 1
name = "com.acme/engineering"
version = "1.2.0"
description = "Acme's reviewed engineering-agent capabilities"

[build]
targets = ["open-agents", "claude"]

[[capabilities]]
path = "capabilities/rust-guidance"

[[capabilities]]
path = "capabilities/security-review"

[[capabilities]]
path = "capabilities/release-prep"
```

## Validate and build

```sh frame="terminal"
tuff pack check .
tuff pack build . --output dist/engineering-1.2.0.tuffpack
tuff pack verify dist/engineering-1.2.0.tuffpack
tuff pack inspect dist/engineering-1.2.0.tuffpack
```

Pack builds validate every capability, ensure workflow requirements are present with the correct types, reject workflow cycles, and confirm every configured adapter supports every member. The artifact contains the verified member sources and pre-rendered target trees. Files and metadata are canonically ordered, so identical input produces identical artifact bytes and SHA-256 digest.

Building, verifying, extracting, and installing a pack never executes member tools or hooks. Runtime dependencies remain the responsibility of the destination environment.

## Extract for runtime infrastructure

Use `pack extract` to produce one harness-native filesystem tree without creating Tuff project state:

```sh frame="terminal"
tuff pack extract dist/engineering-1.2.0.tuffpack \
  --agent open-agents \
  --output runtime-bundle/
```

The output directory must be missing or empty. Tuff verifies the complete artifact before extracting the selected target and never overwrites a non-empty runtime directory.

## Install into a project

Initialize the destination repository and install every member atomically:

```sh frame="terminal"
tuff init
tuff add pack dist/engineering-1.2.0.tuffpack --agent open-agents
tuff list
tuff check
```

Tuff verifies and stages the entire installation before changing the project. A tracked capability or untracked target-file collision prevents every member from being installed. Shared hook and MCP configuration is merged in staging, and a failed commit restores previous files.

Each member remains an ordinary lockfile capability with its normal baseline and drift behavior. Optional pack provenance records the pack name, pack release version, and artifact digest without replacing the member capability version.

## Current boundaries

- Packs contain local manifest-backed members only.
- Pack installation is project-scoped; `--global` is not supported.
- Tuff does not resolve pack-to-pack dependencies or semantic-version constraints.
- Tuff does not install language or system dependencies.
- Registry publishing, signatures, attestations, and policy enforcement are future distribution layers.
