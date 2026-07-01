# Development

Loadout is a Rust CLI project managed with Cargo.

The Docusaurus docs use Node 18 or newer and npm.

## Setup

```sh
cargo fetch
npm ci
```

## Run tests

```sh
cargo test
```

## Run lint

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

## Run all checks

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

If `just` is installed:

```sh
just check
```

## Smoke test an installed binary

Use this flow to test Loadout the way a user would: install the current checkout
as a binary, move to a separate directory, and run `loadout` directly.

```sh
cargo install --path .
mkdir -p /tmp/loadout-smoke
cd /tmp/loadout-smoke
loadout --version
loadout init
loadout add /absolute/path/to/loadout/examples/fixtures/python-uv-default
loadout list
```

From this repo, `just smoke-install` runs the same flow using the fixture path
from the current checkout.

## Build docs

```sh
npm run docs:build
```

## Serve docs locally

```sh
npm run docs:start
```

If `just` is installed:

```sh
just docs-serve
```

## Project structure

```text
src/main.rs        CLI and core implementation
tests/             integration tests
examples/fixtures/ demo and test primitives
docs/              Docusaurus documentation source files
```
