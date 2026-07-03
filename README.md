<p align="center">
  <img src="assets/loadout.png" alt="Loadout logo" width="320">
</p>

# Loadout

Loadout is a CLI for managing project-owned agent capabilities: skills, tools,
hooks, and workflows that teams load into coding harnesses such as Codex,
Claude, Cursor, and others.

This first implementation targets Codex-style skills installed at
`.agents/skills/<id>/SKILL.md`.

Loadout core is intentionally content-agnostic. It is the engine for manifests,
install state, drift detection, diffing, validation, and future harness
adapters. Actual capability content should live in separate pack repositories
or in the projects that own and customize it.

## Documentation

The documentation site is built with Docusaurus:

```sh
npm ci
npm run docs:start
```

Build the static docs site:

```sh
npm run docs:build
```

Start with:

- [Introduction](docs/index.md)
- [Usage Scenarios](docs/usage-scenarios.md)
- [Roadmap](docs/roadmap.md)
- [Skills.sh and Vercel Skills Comparison](docs/comparison/vercel-skills.md)

## Install for local development

From this repository:

```sh
cargo run -- --help
cargo run -- --version
```

To install the CLI from this checkout while developing locally:

```sh
cargo install --path .
loadout --version
```

## Test from another directory

This is the closest local workflow to how an engineer would try Loadout after
installing it:

```sh
cargo install --path .
mkdir -p /tmp/loadout-smoke
cd /tmp/loadout-smoke
loadout --version
loadout init
loadout add /absolute/path/to/loadout/examples/fixtures/python-uv-default
loadout list
```

From this repo, the same smoke test is wrapped as:

```sh
just smoke-install
```

## CLI usage

```sh
loadout
loadout --version
loadout init
loadout add examples/fixtures/python-uv-default
loadout list
loadout diff python-uv-default
```

Running `loadout` with no arguments shows the ASCII banner and starter menu.

`loadout init` only creates `.loadout/lock.json` with empty primitive state. It
does not install defaults or create skills.

The CLI is implemented as a Rust binary crate and commits `Cargo.lock` for
reproducible builds.

The `examples/fixtures/python-uv-default` primitive is a demo/test fixture, not
a bundled standard pack. Production capabilities should live in separate pack
repositories or in the repositories that use them.

## Developer commands

`just` is only a contributor convenience wrapper for this repo, not the user
interface:

```sh
just setup
just check
just run -- --help
```

End users should run `loadout ...` directly.

## Packaging direction

Loadout is now Rust-first. The near-term install path is `cargo install --path .`
for local development. Standalone release binaries and a Homebrew tap should
come after the CLI contract, lockfile format, and capability lifecycle
stabilize.

## License

Loadout is released under the MIT License. See [LICENSE](LICENSE).
