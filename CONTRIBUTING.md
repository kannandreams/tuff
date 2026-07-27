# Contributing to Tuff

Thanks for taking the time to improve Tuff.

Tuff is a Rust CLI for managing project-owned agent capabilities across coding
agent harnesses. Contributions should keep that surface predictable: small CLI
commands, clear errors, reproducible output, and repo-owned files.

## Local Setup

Install a recent Rust toolchain, then fetch dependencies:

```sh
cargo fetch
```

The docs site lives in `website/` and uses npm:

```sh
cd website
npm install
```

## Checks

Run the Rust checks before opening a pull request:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
```

Build the docs site when documentation or landing page files change:

```sh
npm --prefix website run build
```

## Local CLI Testing

Run the CLI from the workspace:

```sh
cargo run -p tuffcli -- --help
cargo run -p tuffcli -- --version
```

Install the CLI from this checkout when testing it from another repository:

```sh
cargo install --path crates/tuff-cli
tuff --version
```

## Contribution Guidelines

- Keep behavior deterministic and friendly to CI.
- Prefer clear, actionable CLI errors over hidden fallback behavior.
- Preserve compatibility with existing lockfiles unless a migration is
  intentional and documented.
- Add or update tests for user-visible command behavior.
- Keep generated files, coverage reports, build outputs, and local caches out
  of commits.

## Pull Requests

Open a pull request with a short summary of what changed, why it changed, and
which checks were run. If the change affects a command, include an example of
the new command output or workflow.
