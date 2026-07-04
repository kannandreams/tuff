---
title: Installation
description: Install and run Coral from source.
---

Coral is currently developed as a Rust CLI.

## Requirements

- Rust and Cargo
- Node 18 or newer
- npm

For project command shortcuts, install `just` as well. The underlying Coral CLI
commands can always be run directly through Cargo.

## Set up from source

Clone the repository, then install dependencies:

```sh
cargo fetch
npm ci
```

Verify the CLI entry point:

```sh
cargo run -- --help
cargo run -- --version
```

Install the local binary from the checkout:

```sh
cargo install --path .
coral --version
```

## Optional command runner

If `just` is installed, use the project recipes:

```sh
just setup
just check
just run -- --help
```

## Package status

Coral does not yet publish release binaries or a Homebrew formula. Until
those exist, use the source checkout with `cargo run -- ...` or install locally
with `cargo install --path .`.
