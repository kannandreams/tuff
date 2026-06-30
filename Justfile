set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    just --list

setup:
    uv sync

test:
    uv run pytest

lint:
    uv run ruff check .

check: lint test

run *args:
    uv run loadout {{args}}

docs-serve:
    uv run mkdocs serve

docs-build:
    uv run mkdocs build --strict
