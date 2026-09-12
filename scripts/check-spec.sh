#!/usr/bin/env bash
set -euo pipefail

# Keeps the published hooks specification in step with the code.
#
# Usage:
#   scripts/check-spec.sh            # verify (CI gate)
#   scripts/check-spec.sh --sync     # regenerate the generated files, then verify
#
# spec/hooks/hooks-spec.json is what `tuff hooks spec --json` prints: the
# canonical events from crates/tuff-hooks-spec and every adapter's
# compatibility matrix. The tables in spec/hooks/SPEC.md between the
# `generated:` markers are rendered from that JSON. Both are committed so a
# reader on GitHub sees them, and this script fails when either drifts from
# the code. The JSON is also validated against spec/hooks/hooks-spec.schema.json,
# with a small validator here rather than a dependency, since the schema uses
# only the common keywords.

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

json="spec/hooks/hooks-spec.json"
schema="spec/hooks/hooks-spec.schema.json"
prose="spec/hooks/SPEC.md"

sync=0
if [[ "${1:-}" == "--sync" ]]; then
  sync=1
elif [[ $# -gt 0 ]]; then
  echo "usage: scripts/check-spec.sh [--sync]" >&2
  exit 2
fi

generated="$(mktemp)"
trap 'rm -f "$generated"' EXIT
cargo run -q -p tuffcli -- hooks spec --json > "$generated"

status=0

if [[ $sync -eq 1 ]]; then
  cp "$generated" "$json"
  echo "synced $json from 'tuff hooks spec --json'"
elif ! cmp -s "$generated" "$json"; then
  cat >&2 <<MSG
error: $json is out of date with the code

  The specification document is generated from crates/tuff-hooks-spec and
  the adapter matrices. Change those, then run:

    mise run spec-sync
MSG
  status=1
fi

python3 - "$json" "$schema" "$prose" "$sync" <<'PY' || status=1
import json
import pathlib
import re
import sys

json_path, schema_path, prose_path = (pathlib.Path(p) for p in sys.argv[1:4])
sync = sys.argv[4] == "1"
errors = []

document = json.loads(json_path.read_text())
schema = json.loads(schema_path.read_text())


# --- a validator for the keywords this schema uses ---------------------------

def resolve(ref):
    node = schema
    for part in ref.removeprefix("#/").split("/"):
        node = node[part]
    return node


def type_ok(value, name):
    return {
        "object": isinstance(value, dict),
        "array": isinstance(value, list),
        "string": isinstance(value, str),
        "boolean": isinstance(value, bool),
        "number": isinstance(value, (int, float)) and not isinstance(value, bool),
        "null": value is None,
    }[name]


def validate(value, node, path):
    if "$ref" in node:
        validate(value, resolve(node["$ref"]), path)
        return
    if "anyOf" in node:
        if not any(check(value, option) for option in node["anyOf"]):
            errors.append(f"{path}: matches none of the allowed forms")
        return
    types = node.get("type")
    if types is not None:
        types = [types] if isinstance(types, str) else types
        if not any(type_ok(value, name) for name in types):
            errors.append(f"{path}: expected {' or '.join(types)}")
            return
    if "enum" in node and value not in node["enum"]:
        errors.append(f"{path}: {value!r} is not one of {node['enum']}")
    if "pattern" in node and isinstance(value, str) and not re.search(node["pattern"], value):
        errors.append(f"{path}: {value!r} does not match {node['pattern']}")
    if isinstance(value, dict):
        for key in node.get("required", []):
            if key not in value:
                errors.append(f"{path}: missing required '{key}'")
        properties = node.get("properties", {})
        for key, item in value.items():
            if key in properties:
                validate(item, properties[key], f"{path}.{key}")
            elif node.get("additionalProperties") is False:
                errors.append(f"{path}: unexpected key '{key}'")
    if isinstance(value, list):
        if "minItems" in node and len(value) < node["minItems"]:
            errors.append(f"{path}: fewer than {node['minItems']} items")
        if "items" in node:
            for index, item in enumerate(value):
                validate(item, node["items"], f"{path}[{index}]")


def check(value, node):
    before = len(errors)
    validate(value, node, "<anyOf>")
    ok = len(errors) == before
    del errors[before:]
    return ok


validate(document, schema, "$")

# --- consistency beyond the schema -------------------------------------------

version = document["spec_version"]
canonical = [event["event"] for event in document["events"]]
for adapter in document["adapters"]:
    listed = [row["event"] for row in adapter["events"]]
    if sorted(listed) != sorted(canonical):
        errors.append(f"adapter {adapter['adapter']}: matrix does not list every event exactly once")
    if adapter["spec_version"] != version:
        errors.append(f"adapter {adapter['adapter']}: targets spec {adapter['spec_version']}, document is {version}")
    for row in adapter["events"]:
        supported = row["coverage"] != "unsupported"
        if supported != (row["native_event"] is not None):
            errors.append(f"adapter {adapter['adapter']} / {row['event']}: native_event must be set exactly when supported")

# --- the prose: version line and generated tables ----------------------------

prose = prose_path.read_text()
if f"\nVersion {version}." not in prose:
    errors.append(f"{prose_path}: does not declare 'Version {version}.'")


def events_table():
    lines = ["| Event | Blocking | Since | Payload fields |", "|---|---|---|---|"]
    for event in document["events"]:
        blocking = event["blocking"]
        blocking = blocking.replace("_", " ") if isinstance(blocking, str) else f"custom: {blocking['custom']}"
        fields = ", ".join(
            f"`{field['name']}`" + ("" if field["required"] else " (optional)")
            for field in event["payload_schema"]["fields"]
        ) or "none"
        lines.append(f"| `{event['event']}` | {blocking} | {event['since_spec_version']} | {fields} |")
    return "\n".join(lines)


def matrices_tables():
    blocks = []
    for adapter in document["adapters"]:
        head = (
            f"### {adapter['display_name']} (`{adapter['adapter']}`)\n\n"
            f"Hooks install under `{adapter['dir_prefix']}/hooks/<id>/`, registered in "
            f"`{adapter['hook_settings_path']}` ({adapter['hook_settings_shape']} shape) as "
            f"`{adapter['hook_command']}`.\n"
        )
        lines = ["| Canonical | Native | Coverage | Aliases | Scope | Caveat |", "|---|---|---|---|---|---|"]
        for row in adapter["events"]:
            native = f"`{row['native_event']}`" if row["native_event"] else "none"
            aliases = ", ".join(f"`{alias}`" for alias in row["aliases"]) or ""
            scope = ", ".join(row["scope"])
            caveat = row["caveat"] or ""
            if row["source"]:
                link = f"[source]({row['source']})"
                caveat = f"{caveat} ({link})" if caveat else link
            lines.append(f"| `{row['event']}` | {native} | {row['coverage']} | {aliases} | {scope} | {caveat} |")
        blocks.append(head + "\n" + "\n".join(lines))
    return "\n\n".join(blocks)


def splice(text, name, body):
    pattern = re.compile(rf"(<!-- generated:{name} -->\n).*?(<!-- /generated:{name} -->)", re.S)
    if not pattern.search(text):
        errors.append(f"{prose_path}: missing generated:{name} markers")
        return text
    return pattern.sub(lambda m: f"{m.group(1)}{body}\n{m.group(2)}", text)


rendered = splice(splice(prose, "events", events_table()), "matrices", matrices_tables())
if rendered != prose:
    if sync:
        prose_path.write_text(rendered)
        print(f"synced generated tables in {prose_path}")
    else:
        errors.append(f"{prose_path}: generated tables are out of date; run 'mise run spec-sync'")

for error in errors:
    print(f"error: {error}", file=sys.stderr)
sys.exit(1 if errors else 0)
PY

if [[ $status -eq 0 ]]; then
  echo "hooks specification OK"
fi
exit $status
