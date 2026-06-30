from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from loadout.errors import LoadoutError

LOCKFILE_VERSION = 1


@dataclass(frozen=True)
class PrimitiveLockEntry:
    id: str
    version: str
    source_path: str
    installed_target_path: str
    baseline_path: str
    baseline_content_hash: str
    installed_content_hash: str


@dataclass(frozen=True)
class Lockfile:
    path: Path
    primitives: dict[str, PrimitiveLockEntry]

    @property
    def root(self) -> Path:
        return self.path.parent.parent


def init_lockfile(repo_root: Path) -> Path:
    loadout_dir = repo_root / ".loadout"
    loadout_dir.mkdir(parents=True, exist_ok=True)
    lock_path = loadout_dir / "lock.json"
    if not lock_path.exists():
        write_lockfile(Lockfile(path=lock_path, primitives={}))
    return lock_path


def require_lockfile(repo_root: Path) -> Lockfile:
    lock_path = repo_root / ".loadout" / "lock.json"
    if not lock_path.exists():
        raise LoadoutError(".loadout/lock.json is missing; run 'loadout init' first")
    return read_lockfile(lock_path)


def read_lockfile(lock_path: Path) -> Lockfile:
    try:
        data = json.loads(lock_path.read_text())
    except json.JSONDecodeError as exc:
        raise LoadoutError(f"invalid lockfile JSON: {exc}") from exc

    if data.get("version") != LOCKFILE_VERSION:
        raise LoadoutError(f"unsupported lockfile version: {data.get('version')!r}")

    raw_primitives = data.get("primitives")
    if not isinstance(raw_primitives, dict):
        raise LoadoutError("lockfile field 'primitives' must be an object")

    primitives = {
        primitive_id: _entry_from_json(primitive_id, raw_entry)
        for primitive_id, raw_entry in raw_primitives.items()
    }
    return Lockfile(path=lock_path, primitives=primitives)


def write_lockfile(lockfile: Lockfile) -> None:
    data: dict[str, Any] = {
        "version": LOCKFILE_VERSION,
        "primitives": {
            primitive_id: {
                "version": entry.version,
                "source_path": entry.source_path,
                "installed_target_path": entry.installed_target_path,
                "baseline_path": entry.baseline_path,
                "baseline_content_hash": entry.baseline_content_hash,
                "installed_content_hash": entry.installed_content_hash,
            }
            for primitive_id, entry in sorted(lockfile.primitives.items())
        },
    }
    lockfile.path.parent.mkdir(parents=True, exist_ok=True)
    lockfile.path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")


def _entry_from_json(primitive_id: str, raw_entry: object) -> PrimitiveLockEntry:
    if not isinstance(raw_entry, dict):
        raise LoadoutError(f"lockfile entry for '{primitive_id}' must be an object")

    fields = (
        "version",
        "source_path",
        "installed_target_path",
        "baseline_path",
        "baseline_content_hash",
        "installed_content_hash",
    )
    missing = [field for field in fields if field not in raw_entry]
    if missing:
        raise LoadoutError(f"lockfile entry for '{primitive_id}' missing: {', '.join(missing)}")

    values = {field: raw_entry[field] for field in fields}
    for field, value in values.items():
        if not isinstance(value, str) or not value:
            raise LoadoutError(
                f"lockfile entry '{primitive_id}.{field}' must be a non-empty string"
            )

    return PrimitiveLockEntry(id=primitive_id, **values)
