#!/usr/bin/env bash
set -euo pipefail

branch="$(git symbolic-ref --quiet --short HEAD 2>/dev/null || true)"

# Shared branches are allowed. Work branches must use an approved type and a
# lowercase, separator-safe branch slug after the slash.
if [[ "$branch" == "main" || "$branch" == "master" || "$branch" == "develop" ]]; then
  exit 0
fi

pattern='^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)/[a-z0-9][a-z0-9-]*$'

if [[ ! "$branch" =~ $pattern ]]; then
  cat >&2 <<'EOF'
Invalid branch name.

Use:
  <type>/<branch-slug>

Allowed types:
  feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert

Examples:
  feat/add-adapter
  feat/sdk-add-adapter
  docs/contributing-guide
  chore/update-deps
EOF
  exit 1
fi
