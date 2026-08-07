<p align="center"><img src="assets/tuff-readme-banner.png" alt="Tuff banner" width="1100" /></p>

# Tuff

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![crates.io](https://img.shields.io/crates/v/tuffcli.svg)](https://crates.io/crates/tuffcli)
[![PyPI](https://img.shields.io/pypi/v/tuffcli.svg)](https://pypi.org/project/tuffcli/)

Tuff is a CLI for managing project-owned agent capabilities, including skills, tools, hooks, and workflows, across coding harnesses such as Codex, Claude Code, Cursor, and Open Agents-compatible tools.

It records capability provenance and a pristine install baseline, making local drift visible and upstream updates reviewable. Capabilities stay versioned with the project while Tuff handles installation, validation, diffs, updates, and harness-specific output.

## Install

On macOS or Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/kannandreams/tuff/main/install.sh | sh
```

Other options:

```sh
cargo install tuffcli       # crates.io
uv tool install tuffcli     # PyPI
pip install tuffcli         # PyPI

brew tap kannandreams/tuff  # Homebrew
brew install tuff
```

## Quick start

```sh
tuff init
tuff create skill my-skill
tuff list

# After editing the generated skill:
tuff diff my-skill
tuff update my-skill
tuff check
```

Use `tuff add` to track an existing local capability or install one from a Git repository. Tuff supports project and global scopes and can emit the same managed capability for multiple agent harnesses.

## Documentation

- [Getting started](https://tuffcli.dev/getting-started/)
- [Installation](https://tuffcli.dev/installation/)
- [CLI reference](https://tuffcli.dev/cli/)

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for project guidelines and [AGENTS.md](AGENTS.md) for repository layout, development commands, and verification guidance. Please follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## License

Tuff is released under the [MIT License](LICENSE).
