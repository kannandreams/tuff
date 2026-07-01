# Development

Loadout is a Python project managed with `uv`.

The Docusaurus docs use Node 18 or newer and npm.

## Setup

```sh
uv sync
npm ci
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
npm run docs:build
```

## Serve docs locally

```sh
npm run docs:start
```

If `just` is installed:

```sh
just docs-serve
```

## Project structure

```text
src/loadout/       CLI and core implementation
tests/             unit and CLI lifecycle tests
examples/fixtures/ demo and test primitives
docs/              Docusaurus documentation source files
```
