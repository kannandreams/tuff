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

docs-serve:
    npm run docs:start

docs-build:
    npm run docs:build
