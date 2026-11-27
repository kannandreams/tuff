#!/usr/bin/env bash
set -euo pipefail

# Keeps the published pack JSON specification in step with the code.
#
# Usage:
#   scripts/check-pack-spec.sh            # verify (CI gate)
#   scripts/check-pack-spec.sh --sync     # regenerate the example, then verify
#
# spec/packs/inspect.example.json is what `tuff pack inspect --json` prints
# for the example pack in spec/packs/example/review-pack. It is committed so
# a reader sees a real document, and this script fails when it drifts from
# the code or stops matching spec/packs/inspect.schema.json.

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

example_pack="spec/packs/example/review-pack"
example="spec/packs/inspect.example.json"
schema="spec/packs/inspect.schema.json"

sync=0
if [[ "${1:-}" == "--sync" ]]; then
  sync=1
elif [[ $# -gt 0 ]]; then
  echo "usage: scripts/check-pack-spec.sh [--sync]" >&2
  exit 2
fi

generated="$(mktemp)"
trap 'rm -f "$generated"' EXIT
cargo run -q -p tuffcli -- pack inspect "$example_pack" --json 2>/dev/null > "$generated"

status=0
if [[ $sync -eq 1 ]]; then
  cp "$generated" "$example"
  echo "synced $example from 'tuff pack inspect --json'"
elif ! cmp -s "$generated" "$example"; then
  cat >&2 <<MSG
error: $example is out of date with the code

  The example is what 'tuff pack inspect --json' prints for $example_pack.
  If the output changed on purpose, update spec/packs/SPEC.md and the
  schema to match, then run:

    mise run spec-sync
MSG
  status=1
fi

python3 -I scripts/json_schema_check.py "$schema" "$example" || status=1

if [[ $status -eq 0 ]]; then
  echo "pack specification OK"
fi
exit $status
