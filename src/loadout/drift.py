from __future__ import annotations

import difflib
from dataclasses import dataclass
from pathlib import Path

from loadout.errors import LoadoutError
from loadout.hash import hash_file
from loadout.lockfile import PrimitiveLockEntry


@dataclass(frozen=True)
class DriftStatus:
    primitive_id: str
    status: str
    current_hash: str | None


def get_drift_status(repo_root: Path, entry: PrimitiveLockEntry) -> DriftStatus:
    target_path = repo_root / entry.installed_target_path
    if not target_path.exists():
        return DriftStatus(entry.id, "missing", None)

    current_hash = hash_file(target_path)
    status = "clean" if current_hash == entry.installed_content_hash else "modified"
    return DriftStatus(entry.id, status, current_hash)


def diff_against_baseline(repo_root: Path, entry: PrimitiveLockEntry) -> str:
    baseline_path = repo_root / entry.baseline_path
    target_path = repo_root / entry.installed_target_path
    if not baseline_path.exists():
        raise LoadoutError(f"baseline file missing for '{entry.id}': {baseline_path}")
    if not target_path.exists():
        raise LoadoutError(f"installed file missing for '{entry.id}': {target_path}")

    baseline_lines = baseline_path.read_text().splitlines(keepends=True)
    target_lines = target_path.read_text().splitlines(keepends=True)
    return "".join(
        difflib.unified_diff(
            baseline_lines,
            target_lines,
            fromfile=f"baseline/{entry.id}/SKILL.md",
            tofile=entry.installed_target_path,
        )
    )
