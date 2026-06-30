# Development

Loadout is a Python project managed with `uv`.

## Setup

```sh
uv sync
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
uv run mkdocs build --strict
```

## Serve docs locally

```sh
uv run mkdocs serve
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
docs/              MkDocs source files
```
