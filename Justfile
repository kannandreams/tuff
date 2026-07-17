set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    just --list

setup:
    cargo fetch
    cd website && npm install

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
    rm -rf /tmp/coral-smoke
    mkdir -p /tmp/coral-smoke
    cd /tmp/coral-smoke && coral --version
    cd /tmp/coral-smoke && coral init
    cd /tmp/coral-smoke && coral add {{justfile_directory()}}/examples/skills/python-uv-default
    cd /tmp/coral-smoke && coral list

docs-serve:
    cd website && npm run dev

docs-build:
    cd website && npm run build

docs-assets:
    ./scripts/generate-doc-screenshot.sh coral-welcome.png -- cargo run --quiet --
