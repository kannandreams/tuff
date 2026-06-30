set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    just --list

setup:
    uv sync
    pnpm install

test:
    uv run pytest

lint:
    uv run ruff check .

check: lint test docs-build

run *args:
    uv run loadout {{args}}

docs-serve:
    pnpm docs:start

docs-build:
    pnpm docs:build
