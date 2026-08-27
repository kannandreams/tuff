---
title: Capability Packs
description: Build, verify, extract, and install immutable bundles of Tuff capabilities.
---

A Tuff pack is a versioned release unit containing one or more agent capabilities. Each skill, tool, hook, or workflow keeps its own capability ID, type, and version; the pack adds a separate name and version for promoting the tested collection as one artifact.

Use packs when a platform team needs to deliver the same reviewed skills, tools, hooks, and workflows into multiple repositories or ephemeral agent runtimes. Continue using `tuff add <capability-type>` when you only need to manage one capability.

:::tip[Learn with a working example]
The [Tuff Pack examples repository](https://github.com/kannandreams/tuff-pack-examples) starts with tracked capabilities and follows them through build, inspection, GHCR publication, pull, extraction, and a container image. It also contains realistic skills, tools, hooks, and workflows.
:::

## Build your project's capabilities

If the capabilities are already tracked by Tuff, you do not need to create `my-first-pack/`, copy files, or write another capability manifest. Run this from the initialized agent project:

```sh frame="terminal"
tuff pack build --name crm-integration
```

Tuff selects all project-scoped tracked capabilities, except the automatically installed `tuff-cli-guide`, and writes:

```text
tuff-dist/crm-integration-0.1.0.tuffpack
```

The default version is `0.1.0`, and the default target is the project's configured default agent. Building does not modify `tuff.lock`, create a reusable pack definition, or overwrite an existing artifact.

Verify what was produced:

```sh frame="terminal"
tuff pack verify tuff-dist/crm-integration-0.1.0.tuffpack
tuff pack inspect tuff-dist/crm-integration-0.1.0.tuffpack
```

This is the normal starting point: add or create capabilities once, test them in the agent project, accept intentional changes with `tuff update`, then package the accepted state.

### Choose capabilities, a version, and targets

Use repeatable selectors when the pack should contain only part of the project. Selecting a workflow automatically includes its tracked transitive requirements.

```sh frame="terminal"
tuff pack build \
  --name crm-integration \
  --version 1.2.0 \
  --capability crm-operating \
  --capability lead-triage \
  --agent open-agents \
  --agent claude
```

An explicit `--capability tuff-cli-guide` includes the guide; only implicit “select all” builds exclude it. Use `--output <path>` when `tuff-dist/` is not appropriate.

### Reuse the same selection

Create a small project-backed definition when a team will build the same pack repeatedly:

```sh frame="terminal"
tuff pack init crm-integration \
  --from-project \
  --version 1.2.0 \
  --capability crm-operating \
  --capability lead-triage
```

Tuff writes `tuff-packs/crm-integration/tuff-pack.toml`. It stores capability IDs, not copied capability directories:

```toml title="tuff-packs/crm-integration/tuff-pack.toml"
schema = 1
name = "crm-integration"
version = "1.2.0"
description = "Project capability pack crm-integration."

[build]
targets = ["open-agents"]

[project]
capabilities = ["crm-operating", "lead-triage"]
```

Workflow requirements are expanded into this list when the definition is created, making review straightforward. Build it with:

```sh frame="terminal"
tuff pack check tuff-packs/crm-integration
tuff pack build tuff-packs/crm-integration
```

The artifact still goes to `tuff-dist/crm-integration-1.2.0.tuffpack` by default.

### What Tuff validates before writing

Project builds read `tuff.lock`, reconstruct portable capability sources, and verify that they reproduce the accepted installed baselines. A selected capability with local drift or a changed source fails with a command explaining that `tuff update <capability>` is required; no artifact is written. Missing requirements, type mismatches, workflow cycles, unsupported targets, and sources that cannot be reconstructed also fail before artifact creation.

Manifest-backed local capabilities and pinned Git skills can be reconstructed. In-place and previously packed skills can be reconstructed from their accepted installed trees. An older pack-installed tool, hook, or workflow whose original manifest is no longer recorded cannot be safely reverse-engineered; reinstall it from a manifest-backed source before repackaging it.

## Advanced: author a standalone source pack

Use a standalone source pack when the capability sources live in a release repository rather than an initialized agent project. This is the lower-level format and remains supported.

```text
crm-integration/
  tuff-pack.toml
  capabilities/
    crm-operating/
      tuff.toml
      SKILL.md
    crm-connector/
      tuff.toml
    pii-guard/
      tuff.toml
      review.sh
    lead-triage/
      tuff.toml
```

Every standalone member must be a local directory beneath the pack root and must contain a valid `tuff.toml`. Remote members, symbolic links, inferred manifests, and paths that escape the pack root are rejected.

```toml title="tuff-pack.toml"
schema = 1
name = "crm-integration"
version = "1.2.0"
description = "Reviewed capabilities for agents that work with CRM data"

[build]
targets = ["open-agents", "claude"]

[[capabilities]]
path = "capabilities/crm-operating"

[[capabilities]]
path = "capabilities/crm-connector"

[[capabilities]]
path = "capabilities/pii-guard"

[[capabilities]]
path = "capabilities/lead-triage"
```

### Validate and build the standalone source

```sh frame="terminal"
tuff pack check .
tuff pack build . --output tuff-dist/crm-integration-1.2.0.tuffpack
tuff pack verify tuff-dist/crm-integration-1.2.0.tuffpack
tuff pack inspect tuff-dist/crm-integration-1.2.0.tuffpack
```

Pack builds validate every capability, ensure workflow requirements are present with the correct types, reject workflow cycles, and confirm every configured adapter supports every member. The artifact contains the verified member sources and pre-rendered target trees. Files and metadata are canonically ordered, so identical input produces identical artifact bytes and SHA-256 digest.

Building, verifying, extracting, and installing a pack never executes member tools or hooks. Runtime dependencies remain the responsibility of the destination environment.

## Publish and pull with OCI

Tuff can store the exact `.tuffpack` bytes in any compatible OCI registry, including GHCR and self-hosted registries. OCI is the distribution protocol used by container registries, but a Tuff pack is a generic OCI artifact rather than a runnable container image.

```sh frame="terminal"
tuff pack push tuff-dist/crm-integration-1.2.0.tuffpack ghcr.io/yourorg/crm-integration:1.2.0
tuff pack pull ghcr.io/yourorg/crm-integration:1.2.0 --output tuff-dist/downloaded.tuffpack
```

An OCI reference has three important parts: the registry (`ghcr.io`), repository (`yourorg/crm-integration`), and either a human-readable tag (`:1.2.0`) or immutable manifest digest (`@sha256:...`). Tuff requires an explicit tag for push and an explicit tag or digest for pull; it never assumes `latest`.

The published object uses an OCI image manifest as a portable envelope with artifact type `application/vnd.tuff.pack.v1`, one layer with media type `application/vnd.tuff.pack.layer.v1`, and the exact `.tuffpack` bytes as that layer. The manifest also carries the pack name, version, and description as standard OCI annotations. It deliberately omits timestamps so the same pack produces the same OCI manifest bytes.

### Two digests, two jobs

| Digest | Identifies | Why it matters |
| --- | --- | --- |
| Artifact digest | The exact `.tuffpack` bytes | Tuff verifies the pack format, canonical metadata, every file, and this digest. |
| Manifest digest | The OCI manifest containing the layer descriptor and annotations | The registry uses this digest as the immutable, pullable OCI reference. |

`pack push` prints both digests and the immutable digest reference. Save that returned reference when a deployment must consume exactly the reviewed release. `pack pull` resolves a tag to its manifest digest before downloading and returns the resolved digest reference in human and JSON output.

### Tags and overwrite safety

Tags are convenient names, but registries allow them to move. Tuff treats an existing tag as immutable by default: pushing the same manifest reports `unchanged`, while pushing different content fails until `--force` is supplied. This check is best-effort because the portable OCI Distribution API does not provide a compare-and-swap operation for tags; concurrent publishers can still race. Use a single publisher and digest-pinned deployment references for release automation.

Pulling never overwrites an existing output file. Tuff downloads into a temporary file beside the destination, checks the OCI layer size and digest, verifies the complete Tuff artifact and its metadata annotations, and only then atomically persists the new file.

### Authentication and TLS

Tuff first uses credentials already configured by `docker login`, then credentials configured by `podman login`, and falls back to anonymous access when neither configuration contains credentials for the registry. Credential helper secrets are not printed in errors. HTTPS is the default; repeat `--ca-file <certificate.pem>` to add private certificate authorities. `--plain-http` disables transport encryption and should only be used with a disposable local development registry.

OCI transport proves that the bytes arrived unchanged; it does not prove who published them. Signatures, attestations, referrer discovery, and trust-policy enforcement remain a later milestone. Store future signatures and attestations as OCI referrers whose `subject` points at the pack manifest digest, without changing the pack object itself.

The Tuff OCI layer and a Docker image filesystem layer are different objects. Docker cannot use a Tuff pack reference in `FROM`; pull and verify the pack with Tuff, extract one harness-native target, then copy that extracted tree into the image. See [OCI Registries and Container Images](/guides/oci-registries-and-container-images/) for a complete Amazon ECR, digest-pinned deployment, and Docker BuildKit walkthrough.

## Extract for runtime infrastructure

Use `pack extract` to produce one harness-native filesystem tree without creating Tuff project state:

```sh frame="terminal"
tuff pack extract tuff-dist/crm-integration-1.2.0.tuffpack \
  --agent open-agents \
  --output runtime-bundle/
```

The output directory must be missing or empty. Tuff verifies the complete artifact before extracting the selected target and never overwrites a non-empty runtime directory.

## Install into a project

Initialize the destination repository and install every member atomically:

```sh frame="terminal"
tuff init
tuff add pack tuff-dist/crm-integration-1.2.0.tuffpack --agent open-agents
tuff list
tuff check
```

Tuff verifies and stages the entire installation before changing the project. A tracked capability or untracked target-file collision prevents every member from being installed. Shared hook and MCP configuration is merged in staging, and a failed commit restores previous files.

Each member remains an ordinary lockfile capability with its normal baseline and drift behavior. Optional pack provenance records the pack name, pack release version, and artifact digest without replacing the member capability version.

## Current boundaries

- A built artifact always contains canonical portable member sources and pre-rendered targets; project-backed selection is an authoring convenience, not a new artifact format.
- Standalone source packs contain local manifest-backed members only.
- Pack installation is project-scoped; `--global` is not supported.
- Tuff does not resolve pack-to-pack dependencies or semantic-version constraints.
- Tuff does not install language or system dependencies.
- Signatures, attestations, referrer discovery, and policy enforcement are future trust layers.
