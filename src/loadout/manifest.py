from __future__ import annotations

import tomllib
from dataclasses import dataclass
from pathlib import Path

from loadout.errors import LoadoutError

SUPPORTED_KIND = "skill"
SUPPORTED_TARGET = "codex"


@dataclass(frozen=True)
class PrimitiveManifest:
    id: str
    version: str
    kind: str
    target: str
    description: str
    files: tuple[str, ...]
    root: Path

    @property
    def skill_source_path(self) -> Path:
        if self.files != ("src/SKILL.md",):
            raise LoadoutError("codex skill primitives must declare files = ['src/SKILL.md']")
        return self.root / "src" / "SKILL.md"


def load_manifest(primitive_dir: Path) -> PrimitiveManifest:
    manifest_path = primitive_dir / "loadout.toml"
    if not manifest_path.exists():
        raise LoadoutError(f"primitive manifest not found: {manifest_path}")

    try:
        data = tomllib.loads(manifest_path.read_text())
    except tomllib.TOMLDecodeError as exc:
        raise LoadoutError(f"invalid primitive manifest TOML: {exc}") from exc

    required = ("id", "version", "kind", "target", "description", "files")
    missing = [key for key in required if key not in data]
    if missing:
        raise LoadoutError(f"primitive manifest missing required fields: {', '.join(missing)}")

    primitive_id = _require_string(data, "id")
    version = _require_string(data, "version")
    kind = _require_string(data, "kind")
    target = _require_string(data, "target")
    description = _require_string(data, "description")
    files = data["files"]
    if not isinstance(files, list) or not all(isinstance(item, str) for item in files):
        raise LoadoutError("primitive manifest field 'files' must be a list of strings")

    if kind != SUPPORTED_KIND:
        raise LoadoutError(
            f"unsupported primitive kind '{kind}'; only '{SUPPORTED_KIND}' is supported"
        )
    if target != SUPPORTED_TARGET:
        raise LoadoutError(
            f"unsupported primitive target '{target}'; only '{SUPPORTED_TARGET}' is supported"
        )

    manifest = PrimitiveManifest(
        id=primitive_id,
        version=version,
        kind=kind,
        target=target,
        description=description,
        files=tuple(files),
        root=primitive_dir,
    )
    if not manifest.skill_source_path.exists():
        raise LoadoutError(f"primitive source file not found: {manifest.skill_source_path}")
    return manifest


def _require_string(data: dict[str, object], key: str) -> str:
    value = data[key]
    if not isinstance(value, str) or not value:
        raise LoadoutError(f"primitive manifest field '{key}' must be a non-empty string")
    return value
