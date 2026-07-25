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
    cargo run -p tuff -- {{args}}

smoke-install:
    cargo install --path crates/tuff-cli
    rm -rf /tmp/tuff-smoke
    mkdir -p /tmp/tuff-smoke
    cd /tmp/tuff-smoke && tuff --version
    cd /tmp/tuff-smoke && tuff init
    cd /tmp/tuff-smoke && tuff add {{justfile_directory()}}/examples/skills/python-uv-default
    cd /tmp/tuff-smoke && tuff list

docs-serve:
    cd website && npm run dev

docs-build:
    cd website && npm run build

docs-assets:
    ./scripts/generate-doc-screenshot.sh tuff-welcome.png -- cargo run -p tuff --quiet --
