<p align="center">
  <img src="assets/coral-readme-banner.png" alt="Coral banner" width="1100" />
</p>

# Coral

Coral is a CLI for managing project-owned agent capabilities: skills, tools,
hooks, and workflows that teams load into coding harnesses such as Codex,
Claude, Cursor, and others.

This first implementation targets Codex-style skills installed at
`.agents/skills/<id>/SKILL.md`.

Coral core is intentionally content-agnostic. It is the engine for manifests,
install state, drift detection, diffing, validation, and future harness
adapters. Actual capability content should live in separate pack repositories
or in the projects that own and customize it.

## Documentation

The documentation site is built with Starlight on Astro in `website/`:

```sh
cd website
npm install
npm run dev
```

Build the static docs site:

```sh
cd website
npm run build
```

Start with:

- [Introduction](website/src/content/docs/index.md)
- [Usage Scenarios](website/src/content/docs/usage-scenarios.md)
- [Roadmap](website/src/content/docs/roadmap.md)
- [Skills.sh and Vercel Skills Comparison](website/src/content/docs/comparison/vercel-skills.md)

## Install for local development

From this repository:

```sh
cargo run -- --help
cargo run -- --version
```

To install the CLI from this checkout while developing locally:

```sh
cargo install --path .
coral --version
```

## Test from another directory

This is the closest local workflow to how an engineer would try Coral after
installing it:

```sh
cargo install --path .
mkdir -p /tmp/coral-smoke
cd /tmp/coral-smoke
coral --version
coral init
coral add /absolute/path/to/coral/examples/fixtures/python-uv-default
coral list
```

From this repo, the same smoke test is wrapped as:

```sh
just smoke-install
```

## CLI usage

```sh
coral
coral --version
coral init
coral add examples/fixtures/python-uv-default
coral list
coral diff python-uv-default
```

Running `coral` with no arguments shows the terminal banner and starter menu.

`coral init` only creates `.coral/lock.json` with empty primitive state. It
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

End users should run `coral ...` directly.

## Packaging direction

Coral is now Rust-first. The near-term install path is `cargo install --path .`
for local development. Standalone release binaries and a Homebrew tap should
come after the CLI contract, lockfile format, and capability lifecycle
stabilize.

## License

Coral is released under the MIT License. See [LICENSE](LICENSE).
