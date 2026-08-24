---
title: Development
description: Contributor setup, checks, and local docs workflows.
---

Tuff uses mise to provision the development toolchain and run project tasks. Install [mise](https://mise.jdx.dev/getting-started.html) before working in the repository.

## Setup

From the repository root:

```sh frame="terminal"
mise run setup
mise run cli -- --help
```

Mise installs the pinned Rust, Node.js, Python, Perl, `pre-commit`, and terminal-screenshot tools declared in `mise.toml`. The setup task also fetches Cargo dependencies, installs website dependencies with `npm ci`, and enables the repository Git hook.

The host still needs Git, a C compiler, and `make`; these are required before mise can clone the repository or compile Tuff's vendored native dependencies.

## Common tasks

| Command | Purpose |
|---|---|
| `mise run cli -- --help` | Run the CLI from source and forward arguments to `tuff` |
| `mise run test` | Run the Rust test suite |
| `mise run lint` | Check Rust formatting and Clippy warnings |
| `mise run security-audit` | Reject known npm dependency vulnerabilities |
| `mise run check` | Run the complete local and CI verification path |
| `mise run smoke-install` | Install the current checkout and exercise it from a clean directory |
| `mise run docs-check` | Type-check the Astro documentation site |
| `mise run docs-build` | Build the documentation site |
| `mise run docs-serve` | Serve the documentation site locally |
| `mise run docs-assets` | Regenerate terminal screenshots with `freeze` |

## Direct commands

Mise is the canonical entry point, but the underlying commands remain available for focused work:

```sh frame="terminal"
cargo test -p tuffcli
cargo test -p tuff-core
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
npm --prefix website run check
npm --prefix website run build
```

## OCI registry integration test

The real push/pull integration test is ignored during ordinary `cargo test` runs because it needs a disposable, anonymous, plain-HTTP OCI registry. Set `TUFF_OCI_TEST_REGISTRY` to the registry host and port, without a URL scheme, and run the ignored test explicitly:

```sh frame="terminal"
docker run --rm -d --name tuff-oci-test -p 5000:5000 registry:2
TUFF_OCI_TEST_REGISTRY=localhost:5000 cargo test -p tuffcli --test oci_registry -- --ignored
docker stop tuff-oci-test
```

The test builds two packs, publishes and repeats an identical push, verifies conflict refusal and forced tag movement, pulls by tag and digest, and compares the downloaded bytes. CI provides `localhost:5000` through its `registry:2` service and sets the same environment variable. Do not point the test at a shared or production registry.

## Project structure

```text
crates/tuff-cli/       CLI commands, packaging, and integration tests
crates/tuff-core/      lifecycle engine and adapter contract
crates/tuff-hooks-spec/ canonical hook events and compatibility types
crates/tuff-adapter-*/ native harness adapters
examples/              runnable capability examples
website/               Astro/Starlight documentation site
```
