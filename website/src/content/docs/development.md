---
title: Development
description: Contributor setup, checks, and local docs workflows.
---

Tuff is a Rust CLI project managed with Cargo.

The Starlight/Astro docs use Node 18 or newer and npm.

## Setup

```sh frame="terminal"
cargo fetch
npm ci
```

## Run tests

```sh frame="terminal"
cargo test
```

## Run lint

```sh frame="terminal"
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Run all checks

```sh frame="terminal"
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

If `just` is installed:

```sh frame="terminal"
just check
```

## Smoke test an installed binary

Use this flow to test Tuff the way a user would: install the current checkout
as a binary, move to a separate directory, and run `tuff` directly.

```sh frame="terminal"
cargo install --path crates/tuff-cli
mkdir -p /tmp/tuff-smoke
cd /tmp/tuff-smoke
tuff --version
tuff init
tuff add /absolute/path/to/tuff/examples/skills/python-uv-default
tuff list
```

From this repo, `just smoke-install` runs the same flow using the skill example
from the current checkout.

## Build docs

```sh frame="terminal"
npm run build
```

## Serve docs locally

```sh frame="terminal"
npm run dev
```

If `just` is installed:

```sh frame="terminal"
just docs-serve
```

CLI screenshots used in the docs can be generated with:

```sh frame="terminal"
just docs-assets
```

This uses `freeze` to capture real command output into `website/public/img/generated/`,
so the same approach can be reused anywhere the docs benefit from a terminal screenshot
instead of inline text.

## Project structure

```text
crates/tuff-cli/  CLI commands and integration tests
crates/tuff-core/ lifecycle engine and adapter contract
crates/tuff-adapter-*/ native harness adapters
tests/             shared test assets when present
examples/          runnable capability examples
docs/              documentation source files
```
