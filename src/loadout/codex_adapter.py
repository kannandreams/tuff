from __future__ import annotations

from pathlib import Path

from loadout.errors import LoadoutError
from loadout.hash import hash_file
from loadout.lockfile import Lockfile, PrimitiveLockEntry, write_lockfile
from loadout.manifest import PrimitiveManifest


def install_codex_skill(repo_root: Path, lockfile: Lockfile, manifest: PrimitiveManifest) -> Path:
    source_path = manifest.skill_source_path
    target_path = repo_root / ".agents" / "skills" / manifest.id / "SKILL.md"
    baseline_path = repo_root / ".loadout" / "baselines" / manifest.id / "SKILL.md"

    existing_entry = lockfile.primitives.get(manifest.id)
    if target_path.exists() and existing_entry is None:
        raise LoadoutError(
            f"refusing to overwrite untracked skill at {target_path}; "
            "remove it or track it in Loadout first"
        )

    content = source_path.read_bytes()
    target_path.parent.mkdir(parents=True, exist_ok=True)
    baseline_path.parent.mkdir(parents=True, exist_ok=True)
    target_path.write_bytes(content)
    baseline_path.write_bytes(content)

    content_hash = hash_file(target_path)
    lockfile.primitives[manifest.id] = PrimitiveLockEntry(
        id=manifest.id,
        version=manifest.version,
        source_path=_relative_to_repo(source_path, repo_root),
        installed_target_path=_relative_to_repo(target_path, repo_root),
        baseline_path=_relative_to_repo(baseline_path, repo_root),
        baseline_content_hash=content_hash,
        installed_content_hash=content_hash,
    )
    write_lockfile(lockfile)
    return target_path


def _relative_to_repo(path: Path, repo_root: Path) -> str:
    try:
        return path.resolve().relative_to(repo_root.resolve()).as_posix()
    except ValueError:
        return str(path.resolve())
