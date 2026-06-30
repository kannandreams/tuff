from pathlib import Path

from loadout.lockfile import (
    PrimitiveLockEntry,
    init_lockfile,
    read_lockfile,
    write_lockfile,
)


def test_init_lockfile_creates_empty_state(tmp_path: Path) -> None:
    lock_path = init_lockfile(tmp_path)

    lockfile = read_lockfile(lock_path)

    assert lock_path == tmp_path / ".loadout" / "lock.json"
    assert lockfile.primitives == {}


def test_write_and_read_lockfile_entry(tmp_path: Path) -> None:
    lock_path = init_lockfile(tmp_path)
    lockfile = read_lockfile(lock_path)
    lockfile.primitives["example"] = PrimitiveLockEntry(
        id="example",
        version="1.0.0",
        source_path="fixtures/example/src/SKILL.md",
        installed_target_path=".agents/skills/example/SKILL.md",
        baseline_path=".loadout/baselines/example/SKILL.md",
        baseline_content_hash="abc",
        installed_content_hash="abc",
    )

    write_lockfile(lockfile)
    loaded = read_lockfile(lock_path)

    assert loaded.primitives["example"].installed_target_path == ".agents/skills/example/SKILL.md"
