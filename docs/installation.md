# Installation

Loadout is currently developed as a local Python CLI managed with `uv`.

## Requirements

- Python 3.12 or newer
- `uv`
- Node 24 LTS
- `pnpm`

For project command shortcuts, install `just` as well. The underlying Python
commands can always be run directly through `uv`.

## Set up from source

Clone the repository, then install dependencies:

```sh
uv sync
pnpm install
```

Verify the CLI entry point:

```sh
uv run loadout --help
```

## Optional command runner

If `just` is installed, use the project recipes:

```sh
just setup
just check
just run -- --help
```

## Package status

Loadout is not yet published to PyPI. Until it is, use the source checkout with
`uv run loadout ...`.
