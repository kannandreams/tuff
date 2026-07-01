set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    just --list

setup:
    cargo fetch
    npm ci

test:
    cargo test

lint:
    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings

check: lint test docs-build

run *args:
    cargo run -- {{args}}

smoke-install:
    cargo install --path .
    rm -rf /tmp/loadout-smoke
    mkdir -p /tmp/loadout-smoke
    cd /tmp/loadout-smoke && loadout --version
    cd /tmp/loadout-smoke && loadout init
    cd /tmp/loadout-smoke && loadout add {{justfile_directory()}}/examples/fixtures/python-uv-default
    cd /tmp/loadout-smoke && loadout list

docs-serve:
    npm run docs:start

docs-build:
    npm run docs:build
