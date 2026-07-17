#!/usr/bin/env bash
set -euo pipefail

check="${1:-status}"
repo="${2:-.}"

case "$check" in
  status)
    git -C "$repo" status --short
    ;;
  rust-tests)
    cargo test --manifest-path "$repo/Cargo.toml"
    ;;
  docs-build)
    npm --prefix "$repo/website" run build
    ;;
  *)
    echo "unsupported check: $check" >&2
    exit 2
    ;;
esac
