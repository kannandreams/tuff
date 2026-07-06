---
title: Installation
description: Install Coral on your machine.
---

## Quick install

:::tabs
:::tab curl

```sh
curl -fsSL https://raw.githubusercontent.com/kannandreams/coral/main/install.sh | sh
```

Downloads the latest release binary for your OS and architecture.

:::

:::tab cargo

```sh
cargo install --git https://github.com/kannandreams/coral
```

Builds from source. Requires Rust and Cargo.

:::
:::

## Verify

```sh
coral --version
coral
```

## Build from source

```sh
git clone https://github.com/kannandreams/coral
cd coral
cargo build --release
./target/release/coral --version
```

## Requirements

- **Rust** — `cargo` uses it directly. `curl` method pre-builds binaries so Rust isn't needed.
- **Git** — required for `coral add` with git-backed skills/tools/hooks.

## What's installed

- `/usr/local/bin/coral` (curl method)
- `~/.coral/cache/git/` — cloned repositories for skill discovery
- `.coral/` — project-level state when you run `coral init`
