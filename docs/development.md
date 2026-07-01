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
