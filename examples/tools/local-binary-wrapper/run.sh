#!/usr/bin/env bash
set -euo pipefail

binary="${1:-git}"
path="${2:-.}"

case "$binary" in
  git)
    git --version
    if git -C "$path" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
      git -C "$path" status --short
    else
      echo "not a git repository: $path"
    fi
    ;;
  rg)
    rg --version | head -n 1
    rg --files "$path" | head -n 25
    ;;
  *)
    echo "unsupported binary: $binary" >&2
    exit 2
    ;;
esac
