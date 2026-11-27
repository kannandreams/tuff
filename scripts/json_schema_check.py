"""Validate a JSON document against a JSON Schema that uses only the common
keywords: type, enum, pattern, required, properties, additionalProperties
(as false), items, minItems, anyOf, and local $ref. Used by
scripts/check-pack-spec.sh so the published schema needs no dependency.

Usage: python3 -I scripts/json_schema_check.py <schema.json> <document.json>
"""

import json
import re
import sys


def main(schema_path, document_path):
    schema = json.load(open(schema_path))
    document = json.load(open(document_path))
    errors = []

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
    for error in errors:
        print(f"error: {document_path}: {error}", file=sys.stderr)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:3]))
