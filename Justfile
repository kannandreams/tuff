set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    just --list

setup:
    uv sync
    npm ci

test:
    uv run pytest

lint:
    uv run ruff check .

check: lint test docs-build

run *args:
    uv run loadout {{args}}

docs-serve:
    npm run docs:start

docs-build:
    npm run docs:build
