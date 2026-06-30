# Development

Loadout is a Python project managed with `uv`.

The Docusaurus docs use Node 24 LTS and `pnpm`.

## Setup

```sh
uv sync
pnpm install
```

## Run tests

```sh
uv run pytest
```

## Run lint

```sh
uv run ruff check .
```

## Run all checks

```sh
uv run ruff check .
uv run pytest
```

If `just` is installed:

```sh
just check
```

## Build docs

```sh
pnpm docs:build
```

## Serve docs locally

```sh
pnpm docs:start
```

If `just` is installed:

```sh
just docs-serve
```

## Project structure

```text
src/loadout/       CLI and core implementation
tests/             unit and CLI lifecycle tests
examples/fixtures/ sample primitives
docs/              Docusaurus documentation source files
```
