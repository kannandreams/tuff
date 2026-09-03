#!/usr/bin/env bash
set -euo pipefail

# Validates the Claude Code plugin and marketplace manifests in this repository.
#
# Usage:
#   scripts/check-plugin.sh            # verify (CI gate)
#   scripts/check-plugin.sh --sync     # regenerate generated plugin files, then verify
#
# The plugin's tuff-cli-guide skill is a generated copy of the CLI asset that
# `tuff init` installs, so the guide has one source of truth. This script fails
# when the copy drifts. When the `claude` binary is available it also runs
# `claude plugin validate --strict` on both manifests; CI has no such binary and
# skips that step.

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

asset="crates/tuff-cli/assets/tuff-cli-guide.md"
generated="plugins/tuff/skills/tuff-cli-guide/SKILL.md"
plugin_manifest="plugins/tuff/.claude-plugin/plugin.json"
marketplace_manifest=".claude-plugin/marketplace.json"

if [[ "${1:-}" == "--sync" ]]; then
  mkdir -p "$(dirname "$generated")"
  cp "$asset" "$generated"
  echo "synced $generated from $asset"
elif [[ $# -gt 0 ]]; then
  echo "usage: scripts/check-plugin.sh [--sync]" >&2
  exit 2
fi

status=0

if ! cmp -s "$asset" "$generated"; then
  cat >&2 <<EOF
error: $generated is out of date with $asset

  The plugin skill is generated from the CLI asset. Edit the asset, then run:

    mise run plugin-sync
EOF
  status=1
fi

python3 - "$plugin_manifest" "$marketplace_manifest" <<'PY' || status=1
import json
import pathlib
import sys

plugin_path, marketplace_path = (pathlib.Path(p) for p in sys.argv[1:3])
errors = []

try:
    plugin = json.loads(plugin_path.read_text())
except (OSError, json.JSONDecodeError) as exc:
    print(f"error: {plugin_path} is unreadable: {exc}", file=sys.stderr)
    sys.exit(1)

try:
    marketplace = json.loads(marketplace_path.read_text())
except (OSError, json.JSONDecodeError) as exc:
    print(f"error: {marketplace_path} is unreadable: {exc}", file=sys.stderr)
    sys.exit(1)

for field in ("name", "version", "description", "license"):
    if not plugin.get(field):
        errors.append(f"{plugin_path}: missing required field '{field}'")

entries = {entry.get("name"): entry for entry in marketplace.get("plugins", [])}
name = plugin.get("name")
if name not in entries:
    errors.append(f"{marketplace_path}: no entry named '{name}'")
else:
    source = entries[name].get("source")
    if not isinstance(source, str):
        errors.append(f"{marketplace_path}: entry '{name}' must use a relative path source")
    elif not (plugin_path.parents[1] == pathlib.Path(source)):
        errors.append(
            f"{marketplace_path}: entry '{name}' points at '{source}', "
            f"not '{plugin_path.parents[1]}'"
        )

for error in errors:
    print(f"error: {error}", file=sys.stderr)
sys.exit(1 if errors else 0)
PY

if command -v claude >/dev/null 2>&1; then
  claude plugin validate ./plugins/tuff --strict || status=1
  claude plugin validate . --strict || status=1
else
  echo "note: 'claude' not on PATH, skipping claude plugin validate"
fi

if [[ $status -eq 0 ]]; then
  echo "plugin manifests OK"
fi

exit $status
