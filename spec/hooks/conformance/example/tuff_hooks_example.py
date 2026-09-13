#!/usr/bin/env python3
"""Clean-room toy implementation of the Tuff-standard hook part of the
Tuff Hooks Specification, version 0.2.0.

Standard library only. All harness-specific data (paths, settings shapes,
compatibility matrices, event vocabulary) is read at runtime from the
``--spec`` JSON document (the machine-readable form described in section 8
of SPEC.md). Nothing about a specific harness is hard-coded here.

See GAPS-0.2.0.md, next to this file, for every place the specification
left a choice unstated and the choice this implementation made, and how
each of the first pass's gaps fared under this version.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tomllib
from pathlib import Path
from typing import Any


class Refusal(Exception):
    """Raised for any condition that must cause a clean, no-write refusal."""


# --------------------------------------------------------------------------
# small helpers
# --------------------------------------------------------------------------

def load_json(path: str | Path) -> Any:
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def write_json(path: Path, data: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def hash_entry(obj: Any) -> str:
    """Section 6.4, "Recording": "the entry hash: the lowercase hexadecimal
    SHA-256 of the entry serialised as compact JSON, with object keys
    sorted, no whitespace, and non-ASCII characters written as UTF-8
    rather than escaped."
    """
    payload = json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


# Section 4.3: "every single quote inside the value written as the five
# characters '"'"' , which closes the quoted string, emits a double-quoted
# single quote, and reopens it."
_ESCAPED_QUOTE = "'\"'\"'"


def sq(value: str) -> str:
    return "'" + value.replace("'", _ESCAPED_QUOTE) + "'"


# --------------------------------------------------------------------------
# spec lookups
# --------------------------------------------------------------------------

def find_adapter(spec: dict, harness_id: str) -> dict | None:
    for adapter in spec.get("adapters", []):
        if adapter.get("adapter") == harness_id:
            return adapter
    return None


def resolve_row(adapter: dict, event_name: str) -> dict | None:
    """Section 5, "Resolution": canonical name first, alias only if no
    canonical row matches. A name that matches an alias on an
    ``unsupported`` row still resolves to that row (and is then refused
    by the caller, per section 5).
    """
    for row in adapter.get("events", []):
        if row.get("event") == event_name:
            return row
    for row in adapter.get("events", []):
        if event_name in (row.get("aliases") or []):
            return row
    return None


def describe_supported(adapter: dict) -> str:
    """Section 5: a refusal message "SHOULD list the canonical events the
    harness supports, with the aliases each accepts."
    """
    parts = []
    for row in adapter.get("events", []):
        if row.get("coverage") == "unsupported":
            continue
        aliases = row.get("aliases") or []
        if aliases:
            parts.append(f"{row['event']} (aliases: {', '.join(aliases)})")
        else:
            parts.append(row["event"])
    return ", ".join(parts) if parts else "(none)"


# --------------------------------------------------------------------------
# manifest (tuff.toml, section 4.1)
# --------------------------------------------------------------------------

class RuntimeFile:
    __slots__ = ("source_rel", "installed_rel")

    def __init__(self, source_rel: str, installed_rel: str):
        self.source_rel = source_rel        # path under MANIFEST_DIR, "./" stripped
        self.installed_rel = installed_rel  # path under the hook directory, one leading src/ stripped


class Manifest:
    def __init__(self, hook_id: str, event: str, command: str, working_directory: str,
                 runtime_files: list[RuntimeFile], toml_path: Path):
        self.hook_id = hook_id
        self.event = event
        self.command = command
        self.working_directory = working_directory
        self.runtime_files = runtime_files
        self.toml_path = toml_path


def _is_plain_path(value: str) -> bool:
    """"a relative path of plain names": one or more '/'-separated
    segments, none empty, '.', or '..'.
    """
    if value == "":
        return False
    return all(seg not in ("", ".", "..") for seg in value.split("/"))


def _has_edge_whitespace(value: str) -> bool:
    """Section 4.1 (revised): "no leading or trailing whitespace, meaning
    any character with the Unicode White_Space property." The standard
    library does not expose that property table directly; `str.isspace()`
    is the closest available proxy (see GAPS-0.2.0.md).
    """
    return bool(value) and (value[0].isspace() or value[-1].isspace())


def validate_plain_path(value: str, what: str) -> None:
    """The shared rule 4.1 states for `id`, and that a `files` entry
    follows "in the same sense as id": no backslash, no NUL, no leading or
    trailing whitespace, and a relative path of plain names (no empty,
    '.', or '..' segment). "These are the only restrictions; any other
    character is allowed."
    """
    if "\x00" in value:
        raise Refusal(f"tuff.toml: {what} {value!r} must not contain a NUL byte")
    if "\\" in value:
        raise Refusal(f"tuff.toml: {what} {value!r} must not contain a backslash")
    if _has_edge_whitespace(value):
        raise Refusal(f"tuff.toml: {what} {value!r} must not have leading or trailing whitespace")
    if not _is_plain_path(value):
        raise Refusal(f"tuff.toml: {what} {value!r} is not a relative path of plain names (section 4.1)")


def validate_id(hook_id: str) -> None:
    if not isinstance(hook_id, str) or hook_id == "":
        raise Refusal("tuff.toml: 'id' is required and must be a non-empty string")
    validate_plain_path(hook_id, "'id'")


def validate_working_directory(value: str) -> None:
    if not isinstance(value, str):
        raise Refusal("tuff.toml: [hook].working_directory must be a string")
    if "\x00" in value:
        raise Refusal("tuff.toml: [hook].working_directory must not contain a NUL byte")
    if value.startswith("/"):
        raise Refusal(f"tuff.toml: [hook].working_directory {value!r} must be a relative path")
    if any(seg == ".." for seg in value.split("/")):
        raise Refusal(f"tuff.toml: [hook].working_directory {value!r} must not contain a '..' segment")


def validate_files_entry(entry: str) -> str:
    """Validates one `files` entry per section 4.1 and returns it with an
    optional leading './' stripped (the path to resolve under
    MANIFEST_DIR, and the key to dedupe repeated entries by -- 4.2:
    "An entry listed more than once, including once with a leading ./
    and once without, is installed once.")."""
    if not isinstance(entry, str) or entry == "":
        raise Refusal("tuff.toml: each 'files' entry must be a non-empty string")
    s = entry[2:] if entry.startswith("./") else entry
    validate_plain_path(s, "files entry")
    return s


def installed_path_for(source_rel: str) -> str:
    """Section 4.2: "Each listed file is copied to the hook directory
    under its listed path, with one leading src/ removed if present."
    """
    parts = source_rel.split("/")
    if parts and parts[0] == "src":
        parts = parts[1:]
    if not parts or any(p == "" for p in parts):
        raise Refusal(f"tuff.toml: files entry {source_rel!r} does not name a valid installed path")
    return "/".join(parts)


def check_no_symlink_and_is_regular_file(manifest_dir: Path, source_rel: str) -> Path:
    """Section 4.1: "no symbolic link at any point along it, naming a
    regular file." Checked component by component so a symlinked
    intermediate directory is caught, not just the final name.
    """
    cur = manifest_dir
    for part in source_rel.split("/"):
        cur = cur / part
        if cur.is_symlink():
            raise Refusal(f"tuff.toml: files entry {source_rel!r} passes through a symbolic link at '{cur}'")
    if not cur.is_file():
        raise Refusal(f"tuff.toml: files entry {source_rel!r} does not name a regular file")
    return cur


def load_manifest(manifest_dir: Path) -> Manifest:
    toml_path = manifest_dir / "tuff.toml"
    if not toml_path.is_file():
        raise Refusal(f"manifest directory '{manifest_dir}' does not contain a tuff.toml")
    try:
        with open(toml_path, "rb") as f:
            data = tomllib.load(f)
    except tomllib.TOMLDecodeError as exc:
        raise Refusal(f"tuff.toml is not valid TOML: {exc}") from exc

    hook_id = data.get("id")
    validate_id(hook_id)

    for key in ("version", "description"):
        v = data.get(key)
        if not isinstance(v, str) or v == "":
            raise Refusal(f"tuff.toml: '{key}' is required and must be a non-empty string")

    if data.get("type") != "hook":
        raise Refusal(f"tuff.toml: 'type' must be \"hook\" (got {data.get('type')!r})")

    hook_tbl = data.get("hook")
    if not isinstance(hook_tbl, dict):
        raise Refusal("tuff.toml: a [hook] table is required")

    event = hook_tbl.get("event")
    if not isinstance(event, str) or event.strip() == "":
        raise Refusal("tuff.toml: [hook].event must be a string that is not empty or only whitespace")

    command = hook_tbl.get("command")
    if not isinstance(command, str) or command.strip() == "":
        raise Refusal("tuff.toml: [hook].command must be a string that is not empty or only whitespace")

    working_directory = hook_tbl.get("working_directory", ".")
    validate_working_directory(working_directory)

    files = data.get("files", [])
    if not isinstance(files, list):
        raise Refusal("tuff.toml: 'files' must be an array of strings")

    runtime_files: list[RuntimeFile] = []
    sources_seen: dict[str, str] = {}      # normalized source_rel -> raw entry text (first seen)
    installed_paths_seen: dict[str, str] = {}  # installed_rel -> normalized source_rel that claimed it
    for raw_entry in files:
        source_rel = validate_files_entry(raw_entry)
        if source_rel in sources_seen:
            # 4.2: "An entry listed more than once, including once with a
            # leading ./ and once without, is installed once." -- not a
            # refusal, just a no-op the second time.
            continue
        sources_seen[source_rel] = raw_entry
        check_no_symlink_and_is_regular_file(manifest_dir, source_rel)
        installed_rel = installed_path_for(source_rel)
        if installed_rel == "run.sh":
            raise Refusal(
                f"tuff.toml: files entry {raw_entry!r} would be installed as 'run.sh', "
                "replacing the hook's wrapper (section 4.2)"
            )
        if installed_rel in installed_paths_seen:
            # 4.2: "MUST refuse a manifest in which two different listed
            # files would be installed under the same path... rather than
            # keep one of them." (distinct from the repeat case above.)
            raise Refusal(
                f"tuff.toml: files entries {installed_paths_seen[installed_rel]!r} and "
                f"{raw_entry!r} would both install to '{installed_rel}'"
            )
        installed_paths_seen[installed_rel] = source_rel
        runtime_files.append(RuntimeFile(source_rel, installed_rel))

    return Manifest(hook_id, event, command, working_directory, runtime_files, toml_path)


# --------------------------------------------------------------------------
# settings merge (section 6)
# --------------------------------------------------------------------------

def default_settings(shape: str) -> dict:
    if shape == "flat":
        return {"version": 1}
    return {}


def load_settings(settings_path: Path, shape: str, native_event: str) -> dict:
    """Section 6.3 (revised): "A missing or empty file is treated as {}
    in the grouped shape and {"version": 1} in the flat shape. A file
    that is not a JSON object, or whose hooks is present and not an
    object, MUST be refused as corrupt and left unchanged. So MUST a file
    in which an event the fragment adds to is present and not an array.
    Events the fragment does not add to are not inspected, and are kept
    exactly as they are, whatever they hold." Only `native_event` -- the
    one this install is about to merge into -- is checked.
    """
    if not settings_path.exists():
        return default_settings(shape)
    text = settings_path.read_text(encoding="utf-8")
    if text.strip() == "":
        return default_settings(shape)
    try:
        data = json.loads(text)
    except json.JSONDecodeError as exc:
        raise Refusal(f"settings file '{settings_path}' is corrupt (not valid JSON): {exc}") from exc
    if not isinstance(data, dict):
        raise Refusal(f"settings file '{settings_path}' is corrupt (not a JSON object)")
    hooks = data.get("hooks")
    if hooks is not None and not isinstance(hooks, dict):
        raise Refusal(f"settings file '{settings_path}' is corrupt ('hooks' is not an object)")
    if isinstance(hooks, dict):
        touched = hooks.get(native_event)
        if touched is not None and not isinstance(touched, list):
            raise Refusal(
                f"settings file '{settings_path}' is corrupt (hooks['{native_event}'] is not an array)"
            )
    return data


def merge_registration(settings: dict, shape: str, native_event: str, command: str) -> tuple[str, Any]:
    """Merge one registration into `settings` in place. Returns
    (entry_hash, entry_written), where the hashed/recorded entry is
    section 6.4's "object carrying the command": the typed entry inside
    the group in the grouped shape, the entry itself in the flat shape.
    """
    hooks = settings.setdefault("hooks", {})
    entry = {"type": "command", "command": command}
    if shape == "grouped":
        arr = hooks.setdefault(native_event, [])
        group = {"hooks": [entry]}
        if group not in arr:
            arr.append(group)
        return hash_entry(entry), entry
    elif shape == "flat":
        settings.setdefault("version", 1)
        flat_entry = {"command": command}
        arr = hooks.setdefault(native_event, [])
        if flat_entry not in arr:
            arr.append(flat_entry)
        return hash_entry(flat_entry), flat_entry
    else:
        raise Refusal(f"unknown hook_settings_shape '{shape}'")


def remove_registration(settings: dict, shape: str, native_event: str, command: str) -> None:
    """Section 6.5, "Removing":
    1. look under the recorded native event.
    2. grouped: remove, from every group's `hooks` array, each entry
       whose `command` equals the recorded command; then remove every
       group whose `hooks` array is now empty.
    3. flat: remove each entry whose `command` equals the recorded
       command.
    4. remove any event left with an empty array.
    """
    hooks = settings.get("hooks")
    if not isinstance(hooks, dict):
        return
    arr = hooks.get(native_event)
    if not isinstance(arr, list):
        return

    if shape == "grouped":
        new_arr = []
        for group in arr:
            if not isinstance(group, dict) or not isinstance(group.get("hooks"), list):
                new_arr.append(group)  # not our shape; leave alone
                continue
            remaining = [e for e in group["hooks"] if not (isinstance(e, dict) and e.get("command") == command)]
            if remaining:
                new_group = dict(group)
                new_group["hooks"] = remaining
                new_arr.append(new_group)
            # else: the group is now empty -> drop it
        hooks[native_event] = new_arr
    else:
        hooks[native_event] = [e for e in arr if not (isinstance(e, dict) and e.get("command") == command)]

    if not hooks[native_event]:
        del hooks[native_event]


# --------------------------------------------------------------------------
# install
# --------------------------------------------------------------------------

def cmd_install(args: argparse.Namespace) -> int:
    spec = load_json(args.spec)
    project_dir = Path(args.project)
    manifest_dir = Path(args.manifest_dir)

    adapter = find_adapter(spec, args.harness)
    if adapter is None:
        known = ", ".join(sorted(a.get("adapter", "?") for a in spec.get("adapters", [])))
        raise Refusal(f"unknown harness '{args.harness}'; known harnesses: {known}")

    # Step 1 (4.4): validate the manifest -- harness-independent.
    manifest = load_manifest(manifest_dir)

    if "\x00" in manifest.command or "\x00" in manifest.working_directory:
        raise Refusal("[hook].command and [hook].working_directory must not contain NUL bytes")

    # Step 2 (4.4): resolve the event against the harness's matrix.
    row = resolve_row(adapter, manifest.event)
    if row is None:
        raise Refusal(
            f"harness '{args.harness}' has no event named '{manifest.event}'; "
            f"it supports: {describe_supported(adapter)}"
        )

    canonical_event = row.get("event")
    coverage = row.get("coverage")
    caveat = row.get("caveat")

    if coverage == "unsupported":
        msg = f"harness '{args.harness}' does not support canonical event '{canonical_event}'."
        if caveat:
            msg += f" {caveat}"
        msg += f" It supports: {describe_supported(adapter)}."
        raise Refusal(msg)

    native_event = row.get("native_event")
    if not native_event:
        raise Refusal(
            f"adapter '{args.harness}' row for '{canonical_event}' has coverage "
            f"'{coverage}' but no native_event; the matrix is malformed"
        )

    dir_prefix = adapter["dir_prefix"]
    hook_dir_rel = f"{dir_prefix}/hooks/{manifest.hook_id}"
    run_sh_rel = f"{hook_dir_rel}/run.sh"

    wrapper = (
        "#!/usr/bin/env bash\n"
        "set -euo pipefail\n"
        f"cd -- {sq(manifest.working_directory)}\n"
        f"exec bash -euo pipefail -c {sq(manifest.command)}\n"
    )

    settings_path_rel = adapter["hook_settings_path"]
    settings_path = project_dir / settings_path_rel
    shape = adapter["hook_settings_shape"]
    registered_command = adapter["hook_command"].replace("<id>", manifest.hook_id)

    # Step 3 (4.4, revised): read the settings file and compute the merged
    # result -- corrupt-settings detection (6.3) happens here, before any
    # write, so a refusal in steps 1-3 leaves the project untouched.
    settings = load_settings(settings_path, shape, native_event)

    entry_hash, _entry = merge_registration(settings, shape, native_event, registered_command)

    warnings: list[str] = []
    if coverage == "partial":
        scope = row.get("scope") or []
        scope_str = ", ".join(scope) if scope else "(unspecified scope)"
        msg = (
            f"partial coverage: canonical event '{canonical_event}' installs as native "
            f"'{native_event}' on '{args.harness}'; scope: {scope_str}."
        )
        if caveat:
            msg += f" {caveat}"
        warnings.append(msg)

    # ---- steps 1-3 are done; nothing has touched PROJECT_DIR yet (4.4) ----

    # Step 4: write the wrapper, the runtime files, and the merged
    # settings file, and record the registration. A failure here (full
    # disk, an unrepresentable name) is an "error rather than a refusal"
    # (4.4): unlike steps 1-3, it need not leave the project untouched,
    # but an implementation SHOULD report which files it had already
    # written.
    written_files: list[str] = []
    try:
        project_dir.mkdir(parents=True, exist_ok=True)
        hook_dir_abs = project_dir / hook_dir_rel
        hook_dir_abs.mkdir(parents=True, exist_ok=True)

        run_sh_abs = project_dir / run_sh_rel
        run_sh_abs.write_text(wrapper, encoding="utf-8")  # mode is not significant (4.3)
        written_files.append(run_sh_rel)

        for rf in manifest.runtime_files:
            src = manifest_dir / rf.source_rel
            dst = hook_dir_abs / rf.installed_rel
            dst.parent.mkdir(parents=True, exist_ok=True)
            dst.write_bytes(src.read_bytes())
            written_files.append(f"{hook_dir_rel}/{rf.installed_rel}")

        write_json(settings_path, settings)
    except OSError as exc:
        msg = f"error while installing hook '{manifest.hook_id}' for harness '{args.harness}': {exc}."
        if written_files:
            msg += f" Files already written: {', '.join(sorted(written_files))}."
        else:
            msg += " No files had been written yet."
        print(msg, file=sys.stderr)
        return 1

    result = {
        "id": manifest.hook_id,
        "files": sorted(written_files),
        "registrations": [
            {
                "settings_path": settings_path_rel,
                "native_event": native_event,
                "canonical_event": canonical_event,
                "command": registered_command,
                "entry_hash": entry_hash,
            }
        ],
        "warnings": warnings,
    }
    print(json.dumps(result))
    for w in warnings:
        print(w, file=sys.stderr)
    return 0


# --------------------------------------------------------------------------
# remove
# --------------------------------------------------------------------------

def cmd_remove(args: argparse.Namespace) -> int:
    spec = load_json(args.spec)
    project_dir = Path(args.project)

    adapter = find_adapter(spec, args.harness)
    if adapter is None:
        known = ", ".join(sorted(a.get("adapter", "?") for a in spec.get("adapters", [])))
        raise Refusal(f"unknown harness '{args.harness}'; known harnesses: {known}")

    regs = load_json(args.registrations)
    if isinstance(regs, dict) and "registrations" in regs:
        regs = regs["registrations"]
    if not isinstance(regs, list):
        raise Refusal("--registrations file must contain a JSON array of registration records")

    shape = adapter["hook_settings_shape"]

    by_path: dict[str, list[dict]] = {}
    for r in regs:
        by_path.setdefault(r["settings_path"], []).append(r)

    # Section 6.5 (revised): "MUST update the settings file before
    # deleting any file, so that if the settings file exists but is not
    # valid JSON, removal stops with the hook's files still in place and
    # the hook still recorded." So every settings file is read and
    # parsed -- refusing on invalid JSON -- *before* any file is written
    # or deleted. "A settings file that does not exist, or that has no
    # `hooks` object, holds nothing to remove": such a file is not an
    # error, just nothing to do for it.
    parsed: dict[str, dict | None] = {}
    for rel_path in by_path:
        settings_path = project_dir / rel_path
        if not settings_path.exists():
            parsed[rel_path] = None
            continue
        text = settings_path.read_text(encoding="utf-8")
        if text.strip() == "":
            parsed[rel_path] = None
            continue
        try:
            data = json.loads(text)
        except json.JSONDecodeError as exc:
            raise Refusal(
                f"settings file '{settings_path}' is not valid JSON; removal stopped, nothing changed: {exc}"
            ) from exc
        if not isinstance(data, dict) or not isinstance(data.get("hooks"), dict):
            parsed[rel_path] = None
            continue
        parsed[rel_path] = data

    # Now that every settings file involved is known to be readable (or
    # absent/empty of hooks), update them...
    for rel_path, entries in by_path.items():
        data = parsed[rel_path]
        if data is None:
            continue
        for r in entries:
            remove_registration(data, shape, r["native_event"], r["command"])
        write_json(project_dir / rel_path, data)

    # ...and only then delete the hook directory (step 5) and prune what
    # removal leaves empty.
    dir_prefix = adapter["dir_prefix"]
    hooks_root = project_dir / dir_prefix / "hooks"
    hook_dir = hooks_root / args.hook_id
    if hook_dir.exists():
        _rmtree(hook_dir)

    # "SHOULD also remove each directory left empty between the hook
    # directory and <dir_prefix>/hooks/, that one included, which for a
    # nested id such as security/format-check includes security/."
    _prune_empty_dirs(hook_dir.parent, hooks_root)

    return 0


def _rmtree(path: Path) -> None:
    if path.is_symlink() or path.is_file():
        path.unlink()
        return
    for child in path.iterdir():
        _rmtree(child)
    path.rmdir()


def _prune_empty_dirs(start: Path, stop_at: Path) -> None:
    """Remove `start` and each of its ancestors up to and including
    `stop_at`, as long as each is empty, stopping at the first
    non-empty directory (section 6.5)."""
    try:
        start.relative_to(stop_at)
    except ValueError:
        return  # `start` is not `stop_at` or a descendant of it
    cur = start
    while True:
        if not cur.is_dir() or any(cur.iterdir()):
            return
        is_stop = cur == stop_at
        cur.rmdir()
        if is_stop:
            return
        cur = cur.parent


# --------------------------------------------------------------------------
# validate-fragment
# --------------------------------------------------------------------------

def cmd_validate_fragment(args: argparse.Namespace) -> int:
    spec = load_json(args.spec)
    adapter = find_adapter(spec, args.harness)
    if adapter is None:
        known = ", ".join(sorted(a.get("adapter", "?") for a in spec.get("adapters", [])))
        raise Refusal(f"unknown harness '{args.harness}'; known harnesses: {known}")

    shape = adapter["hook_settings_shape"]
    frag = load_json(args.fragment)

    if not isinstance(frag, dict):
        raise Refusal("fragment must be a JSON object")

    # 6.2: "whose only top-level key is `hooks`... plus an optional
    # `version` in the flat shape."
    allowed = {"hooks"} if shape == "grouped" else {"hooks", "version"}
    extra = set(frag.keys()) - allowed
    if extra:
        raise Refusal(
            f"fragment has disallowed top-level key(s) {sorted(extra)}; only "
            f"{sorted(allowed)} are permitted for the '{shape}' shape (section 6.2)"
        )

    if "hooks" not in frag:
        raise Refusal("fragment is missing the required 'hooks' key")
    if not isinstance(frag["hooks"], dict):
        raise Refusal("'hooks' must be a JSON object mapping native event names to entries")

    if shape == "flat" and "version" in frag:
        if not isinstance(frag["version"], int) or isinstance(frag["version"], bool):
            raise Refusal("'version' must be an integer")

    # 6.2: "Each event in hooks MUST be an array."
    for event_name, entries in frag["hooks"].items():
        if not isinstance(entries, list):
            raise Refusal(f"hooks['{event_name}'] must be a JSON array")
        # Beyond the letter of 6.2: sanity-check the inner shape too (see GAPS-0.2.0.md).
        for entry in entries:
            if not isinstance(entry, dict):
                raise Refusal(f"hooks['{event_name}'] entries must be JSON objects")
            if shape == "grouped":
                if "hooks" not in entry or not isinstance(entry["hooks"], list):
                    raise Refusal(f"grouped-shape group under '{event_name}' must have a 'hooks' array")
                for inner in entry["hooks"]:
                    if not isinstance(inner, dict) or "command" not in inner:
                        raise Refusal(f"grouped-shape entry under '{event_name}' must have a 'command'")
            else:
                if "command" not in entry:
                    raise Refusal(f"flat-shape entry under '{event_name}' must have a 'command'")

    print("ok")
    return 0


# --------------------------------------------------------------------------
# check-matrices
# --------------------------------------------------------------------------

_SEMVER_RE = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")
_ADAPTER_ID_RE = re.compile(r"^[a-z][a-z0-9-]*$")

_TOP_KEYS = {"spec_version", "events", "adapters"}
_EVENT_KEYS = {"event", "canonical_name", "blocking", "since_spec_version", "payload_schema"}
_FIELD_KEYS = {"name", "value_type", "required", "description"}
_ADAPTER_KEYS = {
    "adapter", "display_name", "dir_prefix", "hook_settings_path",
    "hook_settings_shape", "hook_command", "spec_version", "events",
}
_ROW_KEYS = {
    "event", "native_event", "aliases", "coverage", "scope", "caveat",
    "source", "since_harness_version", "until_harness_version",
}


def cmd_check_matrices(args: argparse.Namespace) -> int:
    spec = load_json(args.spec)
    failures: list[str] = []

    def check(cond: bool, msg: str) -> None:
        if not cond:
            failures.append(msg)

    if not isinstance(spec, dict):
        print("document root must be a JSON object", file=sys.stderr)
        return 1

    check(set(spec.keys()) <= _TOP_KEYS, f"unexpected top-level keys: {sorted(set(spec.keys()) - _TOP_KEYS)}")
    for k in _TOP_KEYS:
        check(k in spec, f"missing required top-level key '{k}'")

    sv = spec.get("spec_version")
    check(isinstance(sv, str) and bool(_SEMVER_RE.match(sv or "")), "spec_version must be a semver string (x.y.z)")

    events = spec.get("events")
    check(isinstance(events, list) and len(events) >= 1, "events must be a non-empty array")

    canonical_names: list[str] = []
    if isinstance(events, list):
        seen: set[str] = set()
        for i, ev in enumerate(events):
            if not isinstance(ev, dict):
                failures.append(f"events[{i}] must be an object")
                continue
            check(set(ev.keys()) <= _EVENT_KEYS, f"events[{i}] has unexpected keys: {sorted(set(ev.keys()) - _EVENT_KEYS)}")
            for k in _EVENT_KEYS:
                check(k in ev, f"events[{i}] missing '{k}'")
            name = ev.get("event")
            check(isinstance(name, str), f"events[{i}].event must be a string")
            check(ev.get("canonical_name") == name, f"events[{i}]: canonical_name ({ev.get('canonical_name')!r}) must equal event ({name!r})")
            if isinstance(name, str):
                check(name not in seen, f"duplicate canonical event '{name}' (item 1: vocabulary)")
                seen.add(name)
                canonical_names.append(name)
            blocking = ev.get("blocking")
            valid_blocking = blocking in ("not_blocking", "blocks_action", "blocks_continuation") or (
                isinstance(blocking, dict) and set(blocking.keys()) == {"custom"} and isinstance(blocking.get("custom"), str)
            )
            check(valid_blocking, f"events[{i}].blocking is not a valid value")
            esv = ev.get("since_spec_version")
            check(isinstance(esv, str) and bool(_SEMVER_RE.match(esv or "")), f"events[{i}].since_spec_version must be semver")
            ps = ev.get("payload_schema")
            ok_ps = isinstance(ps, dict) and set(ps.keys()) <= {"fields"} and "fields" in ps and isinstance(ps.get("fields"), list)
            check(ok_ps, f"events[{i}].payload_schema must be an object with a 'fields' array")
            if isinstance(ps, dict) and isinstance(ps.get("fields"), list):
                for j, fld in enumerate(ps["fields"]):
                    if not isinstance(fld, dict):
                        failures.append(f"events[{i}].payload_schema.fields[{j}] must be an object")
                        continue
                    check(set(fld.keys()) <= _FIELD_KEYS, f"events[{i}].fields[{j}] has unexpected keys")
                    for k in _FIELD_KEYS:
                        check(k in fld, f"events[{i}].fields[{j}] missing '{k}'")
                    check(isinstance(fld.get("name"), str), f"events[{i}].fields[{j}].name must be a string")
                    check(fld.get("value_type") in ("string", "boolean", "number", "object", "array"), f"events[{i}].fields[{j}].value_type invalid")
                    check(isinstance(fld.get("required"), bool), f"events[{i}].fields[{j}].required must be a boolean")
                    check(isinstance(fld.get("description"), str), f"events[{i}].fields[{j}].description must be a string")

    canonical_set = set(canonical_names)

    adapters = spec.get("adapters")
    check(isinstance(adapters, list), "adapters must be an array")
    if isinstance(adapters, list):
        for ai, ad in enumerate(adapters):
            if not isinstance(ad, dict):
                failures.append(f"adapters[{ai}] must be an object")
                continue
            adapter_id = ad.get("adapter", f"#{ai}")
            check(set(ad.keys()) <= _ADAPTER_KEYS, f"adapters[{ai}] has unexpected keys: {sorted(set(ad.keys()) - _ADAPTER_KEYS)}")
            for k in _ADAPTER_KEYS:
                check(k in ad, f"adapters[{ai}] missing '{k}'")
            check(isinstance(ad.get("adapter"), str) and bool(_ADAPTER_ID_RE.match(ad.get("adapter") or "")), f"adapters[{ai}].adapter must match ^[a-z][a-z0-9-]*$")
            check(ad.get("hook_settings_shape") in ("grouped", "flat"), f"adapter '{adapter_id}': hook_settings_shape must be 'grouped' or 'flat'")
            asv = ad.get("spec_version")
            check(isinstance(asv, str) and bool(_SEMVER_RE.match(asv or "")), f"adapter '{adapter_id}': spec_version must be semver")

            rows = ad.get("events")
            check(isinstance(rows, list), f"adapter '{adapter_id}': events must be an array")
            row_names: list[str] = []
            if isinstance(rows, list):
                for ri, row in enumerate(rows):
                    if not isinstance(row, dict):
                        failures.append(f"adapter '{adapter_id}': events[{ri}] must be an object")
                        continue
                    check(set(row.keys()) <= _ROW_KEYS, f"adapter '{adapter_id}': events[{ri}] has unexpected keys")
                    for k in _ROW_KEYS:
                        check(k in row, f"adapter '{adapter_id}': events[{ri}] missing '{k}'")
                    ev_name = row.get("event")
                    check(ev_name in canonical_set, f"adapter '{adapter_id}': events[{ri}].event '{ev_name}' is not a declared canonical event (item 1)")
                    if isinstance(ev_name, str):
                        row_names.append(ev_name)
                    coverage = row.get("coverage")
                    check(coverage in ("full", "partial", "unsupported"), f"adapter '{adapter_id}': events[{ri}].coverage invalid")
                    native = row.get("native_event")
                    if coverage == "unsupported":
                        check(native is None, f"adapter '{adapter_id}' row '{ev_name}': unsupported coverage MUST NOT name a native_event (item 4 / section 5)")
                    elif coverage in ("full", "partial"):
                        check(isinstance(native, str) and bool(native), f"adapter '{adapter_id}' row '{ev_name}': coverage '{coverage}' MUST name a native_event (item 4 / section 5)")
                    check(isinstance(row.get("aliases"), list), f"adapter '{adapter_id}': events[{ri}].aliases must be an array")
                    check(isinstance(row.get("scope"), list), f"adapter '{adapter_id}': events[{ri}].scope must be an array")
                    check(row.get("caveat") is None or isinstance(row.get("caveat"), str), f"adapter '{adapter_id}': events[{ri}].caveat must be string or null")
                    check(row.get("source") is None or isinstance(row.get("source"), str), f"adapter '{adapter_id}': events[{ri}].source must be string or null")
                    check(row.get("since_harness_version") is None or isinstance(row.get("since_harness_version"), str), f"adapter '{adapter_id}': events[{ri}].since_harness_version must be string or null")
                    check(row.get("until_harness_version") is None or isinstance(row.get("until_harness_version"), str), f"adapter '{adapter_id}': events[{ri}].until_harness_version must be string or null")

                counts: dict[str, int] = {}
                for n in row_names:
                    counts[n] = counts.get(n, 0) + 1
                for n in canonical_set:
                    c = counts.get(n, 0)
                    check(c == 1, f"adapter '{adapter_id}' covers canonical event '{n}' {c} time(s); must be exactly once (item 3: matrix completeness)")
                extra_names = set(row_names) - canonical_set
                check(not extra_names, f"adapter '{adapter_id}' has rows for unknown canonical events: {sorted(extra_names)}")

    if failures:
        for f in failures:
            print(f, file=sys.stderr)
        return 1
    print("ok")
    return 0


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------

def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="tuff_hooks_example.py")
    sub = p.add_subparsers(dest="cmd", required=True)

    install_p = sub.add_parser("install")
    install_p.add_argument("--spec", required=True)
    install_p.add_argument("--project", required=True)
    install_p.add_argument("--harness", required=True)
    install_p.add_argument("manifest_dir")
    install_p.set_defaults(func=cmd_install)

    remove_p = sub.add_parser("remove")
    remove_p.add_argument("--spec", required=True)
    remove_p.add_argument("--project", required=True)
    remove_p.add_argument("--harness", required=True)
    remove_p.add_argument("--registrations", required=True)
    remove_p.add_argument("hook_id")
    remove_p.set_defaults(func=cmd_remove)

    validate_p = sub.add_parser("validate-fragment")
    validate_p.add_argument("--spec", required=True)
    validate_p.add_argument("--harness", required=True)
    validate_p.add_argument("fragment")
    validate_p.set_defaults(func=cmd_validate_fragment)

    check_p = sub.add_parser("check-matrices")
    check_p.add_argument("--spec", required=True)
    check_p.set_defaults(func=cmd_check_matrices)

    return p


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        return args.func(args)
    except Refusal as exc:
        print(str(exc), file=sys.stderr)
        return 1
    except (FileNotFoundError, json.JSONDecodeError, KeyError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
