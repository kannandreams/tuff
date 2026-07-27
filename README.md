<p align="center">
  <img src="assets/tuff-readme-banner.png" alt="Tuff banner" width="1100" />
</p>

# Tuff

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Coverage](https://img.shields.io/badge/coverage-79%25-brightgreen)](https://github.com/kannandreams/tuff)

Tuff is a CLI for managing project-owned agent capabilities: skills, tools,
hooks, and workflows that teams load into coding harnesses such as Codex,
Claude, Cursor, and others.

Tuff keeps capability files in the project while tracking the metadata around
them: source, version, agent harness, scope, and a pristine install baseline.
That makes local customization visible instead of turning it into an
untracked copy.

The core is content-agnostic. Capability content can live in the repository
that owns it or in a separate pack repository. Tuff handles manifests,
installation, validation, drift detection, diffs, updates, and agent-specific
emission. The workspace contains dedicated Claude, Codex, Cursor, and Open
Agents adapter crates; each owns its native output and compatibility data.

## How the lifecycle works

1. Create a new capability or add an existing one.
2. Track it for one or more agent harnesses.
3. Tuff records install identity in `tuff.lock` and keeps verified materialized
   baselines in the disposable machine-local cache.
4. Edit the project-owned files normally; `tuff list`, `tuff status`, and
   `tuff diff` report drift.
5. Use `tuff update` to accept intentional local changes or refresh from the
   recorded source.

Project and global scopes are supported. Project capabilities take precedence
when the same id exists in both scopes, and Tuff reports that relationship in
status output.

## Install

For the latest released version on macOS or Linux, use the install script:

```sh
curl -fsSL https://raw.githubusercontent.com/kannandreams/tuff/main/install.sh | sh
```

From crates.io:

```sh
cargo install tuffcli
```

From PyPI:

```sh
pip install tuffcli
```

Or install the isolated CLI with uv:

```sh
uv tool install tuffcli
```

On macOS with Homebrew:

```sh
brew tap kannandreams/tuff
brew install tuff
```

See the [latest GitHub release](https://github.com/kannandreams/tuff/releases/latest)
for release notes and platform artifacts.

### Build from source

From this repository:

```sh
cargo run -p tuffcli -- --help
cargo run -p tuffcli -- --version
```

To install the CLI from this checkout while developing locally:

```sh
cargo install --path crates/tuff-cli
tuff --version
```

The repository root is a virtual Cargo workspace, so `cargo install --path .`
is not a valid install command. The CLI package lives at
`crates/tuff-cli`.

The repository requires a recent Rust toolchain with Cargo. There is currently
no dependency on a separately installed JavaScript runtime for the CLI itself.

## Test from another directory

This is the closest local workflow to how an engineer would try Tuff after
installing it:

```sh
cargo install --path crates/tuff-cli
mkdir -p /tmp/tuff-smoke
cd /tmp/tuff-smoke
tuff --version
tuff init
tuff agent add open-agents
tuff add /absolute/path/to/tuff/examples/skills/python-uv-default
tuff list
```

From this repo, the same smoke test is wrapped as:

```sh
just smoke-install
```

## CLI usage

```sh
tuff
tuff --version
tuff init
tuff agent add open-agents
tuff add examples/skills/python-uv-default
tuff list
tuff diff python-uv-default
tuff delete python-uv-default
```

Running `tuff` with no arguments shows the terminal banner and starter menu.

`tuff init` creates `tuff.lock`, configures `open-agents` as the default, registers it, and scaffolds the standard `.agents/`
directories, and installs the small `tuff-cli-guide` reference skill. It does
not install third-party capabilities or create a user skill for you.

Capability cleanup is explicit. Use `tuff delete <id>` for the configured
default agent, or `tuff delete <id> -a <agent>` for a specific agent. Use
`tuff untrack <id>` when the files should remain in place but no longer be
managed by Tuff. `tuff agent remove <agent>`
only unregisters an agent and does not remove capabilities.

Set the default project agent with `tuff agent set-default <agent>`. Use
`--global` to configure the default used by global operations. Explicit
`-a/--agent` values always override the default.

The CLI is built from the Rust crate in this repository, and `Cargo.lock` is
committed for reproducible builds.

The `examples/skills/python-uv-default` capability is a local demonstration,
not a bundled standard pack. Production capabilities should live in separate
pack repositories or in the repositories that use them.

## Developer commands

`just` is only a contributor convenience wrapper for this repo, not the user
interface:

```sh
just setup
just check
just run -- --help
```

End users should run `tuff ...` directly.

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

- [Intro](website/src/content/docs/intro.mdx)
- [When to Use Tuff](website/src/content/docs/usage-scenarios.md)
- [Skills.sh and Vercel Skills Comparison](website/src/content/docs/comparison/vercel-skills.md)

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for local
setup, checks, and contribution guidelines.

## Code of Conduct

This project follows the [Code of Conduct](CODE_OF_CONDUCT.md). Please keep
issues, discussions, and pull requests respectful and constructive.

## License

Tuff is released under the MIT License. See [LICENSE](LICENSE).
