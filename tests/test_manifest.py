from pathlib import Path

import pytest

from loadout.errors import LoadoutError
from loadout.manifest import load_manifest


def test_loads_valid_manifest(tmp_path: Path) -> None:
    primitive = tmp_path / "primitive"
    (primitive / "src").mkdir(parents=True)
    (primitive / "loadout.toml").write_text(
        "\n".join(
            [
                'id = "example"',
                'version = "1.0.0"',
                'kind = "skill"',
                'target = "codex"',
                'description = "Example primitive."',
                'files = ["src/SKILL.md"]',
            ]
        )
    )
    (primitive / "src" / "SKILL.md").write_text("# Example\n")

    manifest = load_manifest(primitive)

    assert manifest.id == "example"
    assert manifest.skill_source_path == primitive / "src" / "SKILL.md"


def test_rejects_unsupported_target(tmp_path: Path) -> None:
    primitive = tmp_path / "primitive"
    (primitive / "src").mkdir(parents=True)
    (primitive / "loadout.toml").write_text(
        "\n".join(
            [
                'id = "example"',
                'version = "1.0.0"',
                'kind = "skill"',
                'target = "claude-code"',
                'description = "Example primitive."',
                'files = ["src/SKILL.md"]',
            ]
        )
    )
    (primitive / "src" / "SKILL.md").write_text("# Example\n")

    with pytest.raises(LoadoutError, match="unsupported primitive target"):
        load_manifest(primitive)
