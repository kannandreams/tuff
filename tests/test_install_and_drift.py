from pathlib import Path

import pytest

from loadout.codex_adapter import install_codex_skill
from loadout.drift import diff_against_baseline, get_drift_status
from loadout.errors import LoadoutError
from loadout.lockfile import init_lockfile, read_lockfile
from loadout.manifest import load_manifest


def make_primitive(tmp_path: Path, primitive_id: str = "example") -> Path:
    primitive = tmp_path / "primitive"
    (primitive / "src").mkdir(parents=True)
    (primitive / "loadout.toml").write_text(
        "\n".join(
            [
                f'id = "{primitive_id}"',
                'version = "1.0.0"',
                'kind = "skill"',
                'target = "codex"',
                'description = "Example primitive."',
                'files = ["src/SKILL.md"]',
            ]
        )
    )
    (primitive / "src" / "SKILL.md").write_text("# Example\n\nOriginal text.\n")
    return primitive


def test_install_codex_skill_and_detect_clean_drift(tmp_path: Path) -> None:
    lock_path = init_lockfile(tmp_path)
    lockfile = read_lockfile(lock_path)
    manifest = load_manifest(make_primitive(tmp_path))

    target_path = install_codex_skill(tmp_path, lockfile, manifest)
    loaded = read_lockfile(lock_path)
    status = get_drift_status(tmp_path, loaded.primitives["example"])

    assert target_path == tmp_path / ".agents" / "skills" / "example" / "SKILL.md"
    assert target_path.read_text() == "# Example\n\nOriginal text.\n"
    assert status.status == "clean"


def test_install_refuses_to_overwrite_untracked_skill(tmp_path: Path) -> None:
    lock_path = init_lockfile(tmp_path)
    lockfile = read_lockfile(lock_path)
    manifest = load_manifest(make_primitive(tmp_path))
    existing = tmp_path / ".agents" / "skills" / "example" / "SKILL.md"
    existing.parent.mkdir(parents=True)
    existing.write_text("# Existing\n")

    with pytest.raises(LoadoutError, match="refusing to overwrite untracked skill"):
        install_codex_skill(tmp_path, lockfile, manifest)


def test_detects_modified_drift_and_diff(tmp_path: Path) -> None:
    lock_path = init_lockfile(tmp_path)
    lockfile = read_lockfile(lock_path)
    manifest = load_manifest(make_primitive(tmp_path))
    target_path = install_codex_skill(tmp_path, lockfile, manifest)
    target_path.write_text("# Example\n\nChanged text.\n")
    loaded = read_lockfile(lock_path)
    entry = loaded.primitives["example"]

    status = get_drift_status(tmp_path, entry)
    diff = diff_against_baseline(tmp_path, entry)

    assert status.status == "modified"
    assert "-Original text." in diff
    assert "+Changed text." in diff
