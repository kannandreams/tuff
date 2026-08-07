# Tuff repository guidance

This file contains implementation guidance for coding agents working in this checkout. Keep the root README concise and user-facing, detailed product documentation in `website/`, and contribution policy in `CONTRIBUTING.md`.

## Repository layout

- `crates/tuff-cli/`: CLI commands, packaging, assets, and integration tests.
- `crates/tuff-core/`: lifecycle engine and adapter contract.
- `crates/tuff-hooks-spec/`: canonical hook events and compatibility types.
- `crates/tuff-adapter-*/`: harness-specific rendering and compatibility.
- `examples/`: local capabilities and integration examples.
- `website/`: Astro/Starlight documentation site.

The repository root is a virtual Cargo workspace. The CLI package is `tuffcli`, and its binary is `tuff`.

## Setup and local use

Use a recent stable Rust toolchain. Documentation development requires Node.js 18.17 or newer and npm. `just` is optional but provides the standard wrappers:

```sh
just setup
just run -- --help
```

Without `just`:

```sh
cargo fetch
npm --prefix website install
cargo run -p tuffcli -- --help
```

To install the current checkout, use `cargo install --path crates/tuff-cli`. `cargo install --path .` does not work because the root has no package.

## Verification

Run the narrowest checks that cover the change. The full repository check is:

```sh
just check
```

Equivalent commands and useful targeted checks:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo test -p tuffcli
cargo test -p tuff-core
npm --prefix website run check
npm --prefix website run build
```

For installation or end-to-end CLI changes, also run:

```sh
just smoke-install
```

Serve the documentation with `just docs-serve`. Regenerate terminal screenshots with `just docs-assets` when their underlying CLI output changes.

## Change guidelines

- Keep changes scoped to the owning crate or adapter.
- Treat CLI behavior, emitted files, manifests, and `tuff.lock` compatibility as user-facing contracts.
- Keep behavior deterministic and errors actionable; do not silently overwrite user-managed content.
- Put harness-specific behavior in the corresponding adapter crate.
- Add or update focused tests for user-visible behavior.
- Update the website or README when public commands or workflows change.
- Keep each Markdown paragraph on a single source line; do not hard-wrap prose.
- Do not commit build outputs, caches, or machine-specific configuration.
