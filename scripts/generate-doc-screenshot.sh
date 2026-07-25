#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/generate-doc-screenshot.sh <output-file> [--cwd <dir>] -- <command...>

Examples:
  scripts/generate-doc-screenshot.sh tuff-welcome.png -- tuff
  scripts/generate-doc-screenshot.sh tuff-list.png --cwd /tmp/demo -- tuff list

Output files are written under website/public/img/generated/.
Requires: freeze
EOF
}

if [[ "${1:-}" == "" || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if ! command -v freeze >/dev/null 2>&1; then
  echo "error: freeze is not installed." >&2
  echo "install it with: brew install charmbracelet/tap/freeze" >&2
  exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_name="$1"
shift

capture_cwd="$repo_root"
if [[ "${1:-}" == "--cwd" ]]; then
  capture_cwd="$2"
  shift 2
fi

if [[ "${1:-}" != "--" ]]; then
  echo "error: expected '--' before the command to capture" >&2
  usage >&2
  exit 1
fi
shift

if [[ "$#" -eq 0 ]]; then
  echo "error: missing command to capture" >&2
  usage >&2
  exit 1
fi

mkdir -p "$repo_root/website/public/img/generated"
output_path="$repo_root/website/public/img/generated/$output_name"

printf -v quoted_cmd '%q ' "$@"
execute_cmd="${quoted_cmd% }"
if [[ "$capture_cwd" != "$repo_root" ]]; then
  printf -v execute_cmd 'cd %q && %s' "$capture_cwd" "$execute_cmd"
fi

freeze --execute "env -u NO_COLOR ${execute_cmd}" --output "$output_path"

echo "generated $output_path"
