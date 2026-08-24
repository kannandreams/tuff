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

## Publish and pull with OCI

Tuff can store the exact `.tuffpack` bytes in any compatible OCI registry, including GHCR and self-hosted registries. OCI is the distribution protocol used by container registries, but a Tuff pack is a generic OCI artifact rather than a runnable container image.

```sh frame="terminal"
tuff pack push dist/engineering-1.2.0.tuffpack ghcr.io/acme/engineering:1.2.0
tuff pack pull ghcr.io/acme/engineering:1.2.0 --output dist/downloaded.tuffpack
```

An OCI reference has three important parts: the registry (`ghcr.io`), repository (`acme/engineering`), and either a human-readable tag (`:1.2.0`) or immutable manifest digest (`@sha256:...`). Tuff requires an explicit tag for push and an explicit tag or digest for pull; it never assumes `latest`.

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
- Signatures, attestations, referrer discovery, and policy enforcement are future trust layers.
