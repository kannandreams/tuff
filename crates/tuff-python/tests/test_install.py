"""Installing, updating, and removing a pack from Python, the way an
application does: a workspace and a baseline folder the caller names, and a
plan shown before anything is written."""

import json
import shutil
from pathlib import Path

import pytest

import tuff_sdk as tuff

EXAMPLE_PACK = Path(__file__).resolve().parents[3] / "spec/packs/example/review-pack"


def files_under(root: Path) -> list[str]:
    return sorted(
        path.relative_to(root).as_posix() for path in root.rglob("*") if path.is_file()
    )


@pytest.fixture
def workspace(tmp_path: Path) -> tuff.Workspace:
    return tuff.Workspace.init(tmp_path / "ws")


@pytest.fixture
def pack() -> tuff.Pack:
    return tuff.Pack.build(EXAMPLE_PACK)


def test_pack_is_read_and_inspected(pack: tuff.Pack) -> None:
    assert pack.name == "com.example/review"
    assert pack.version == "1.0.0"
    assert pack.digest
    document = pack.inspect()
    assert document["name"] == "com.example/review"
    assert document["source"] == {"kind": "folder", "path": str(EXAMPLE_PACK)}
    assert any(member["runsCode"] for member in document["members"])
    assert {member["id"] for member in document["members"]} == {
        "github",
        "lint-hook",
        "review-skill",
    }
    assert repr(pack) == "<Pack com.example/review 1.0.0>"


def test_plan_writes_nothing(workspace: tuff.Workspace, pack: tuff.Pack) -> None:
    lock_before = (workspace.root / "tuff.lock").read_bytes()
    plan = workspace.plan_install(pack, ["claude"])
    assert plan.operation == "install"
    assert plan.pack == "com.example/review"
    assert plan.release.version == "1.0.0"
    assert plan.installed is None
    assert sorted((member.id, member.change) for member in plan.members) == [
        ("github", "add"),
        ("lint-hook", "add"),
        ("review-skill", "add"),
    ]
    changes = {file.path: file.change for file in plan.files}
    assert changes["tuff.lock"] == "modify"
    assert changes[".claude/skills/review-skill/SKILL.md"] == "create"
    assert any("echo lint" in note for note in plan.notes)
    assert plan.fingerprint.startswith("sha256:")
    document = plan.to_dict()
    assert document["operation"] == "install"
    assert document["fingerprint"] == plan.fingerprint
    assert json.loads(plan.to_json()) == document
    assert files_under(workspace.root) == ["tuff.lock"]
    assert (workspace.root / "tuff.lock").read_bytes() == lock_before


def test_install_update_and_remove(
    tmp_path: Path, workspace: tuff.Workspace, pack: tuff.Pack
) -> None:
    baselines = tuff.Baselines.dir(tmp_path / "baselines")
    plan = workspace.plan_install(pack, ["claude"])
    planned = [file.path for file in plan.files]
    summary = workspace.apply(plan, baselines)
    assert summary.pack == "com.example/review"
    assert files_under(workspace.root) == planned
    assert files_under(tmp_path / "baselines")

    # A plan is applied once.
    assert plan.used
    with pytest.raises(tuff.PlanUsedError):
        workspace.apply(plan)

    # The installed release again: nothing to plan.
    assert workspace.plan_update("com.example/review", pack) is None

    # An edited member blocks a removal without force.
    skill = workspace.root / ".claude/skills/review-skill/SKILL.md"
    original = skill.read_bytes()
    skill.write_bytes(b"edited\n")
    with pytest.raises(tuff.DriftError) as raised:
        workspace.plan_remove("com.example/review")
    assert raised.value.kind == "drift"
    assert isinstance(raised.value, tuff.TuffError)
    skill.write_bytes(original)

    plan = workspace.plan_remove("com.example/review")
    assert plan.operation == "remove"
    assert {member.change for member in plan.members} == {"remove"}
    workspace.apply(plan)
    assert not any("review-skill" in path for path in files_under(workspace.root))


def test_drift_and_lock_refuse_apply(workspace: tuff.Workspace, pack: tuff.Pack) -> None:
    lock = workspace.root / "tuff.lock"
    lock_before = lock.read_bytes()
    plan = workspace.plan_install(pack, ["claude"])
    lock.write_text("{}\n")
    with pytest.raises(tuff.DriftError) as raised:
        workspace.apply(plan)
    assert raised.value.hint == "make the plan again"
    lock.write_bytes(lock_before)


def test_approval_after_a_restart(
    tmp_path: Path, workspace: tuff.Workspace, pack: tuff.Pack
) -> None:
    # Only the fingerprint is kept with the approval.
    approved = workspace.plan_install(pack, ["claude"]).fingerprint
    assert workspace.plan_install(pack, ["claude"]).fingerprint == approved

    # The author changed the pack since, keeping its version.
    changed_folder = tmp_path / "changed-pack"
    shutil.copytree(EXAMPLE_PACK, changed_folder)
    skill = changed_folder / "capabilities/review-skill/SKILL.md"
    skill.write_text(skill.read_text() + "\nOne more step.\n")
    changed_pack = tuff.Pack.build(changed_folder)
    result = workspace.apply_approved(
        workspace.plan_install(changed_pack, ["claude"]), approved
    )
    assert isinstance(result, tuff.Changed)
    assert result.plan.fingerprint != approved
    assert not result.plan.used
    assert files_under(workspace.root) == ["tuff.lock"]

    # The approved change, planned again, is applied.
    result = workspace.apply_approved(workspace.plan_install(pack, ["claude"]), approved)
    assert isinstance(result, tuff.Applied)
    assert result.summary.pack == "com.example/review"


def test_errors_carry_kind_and_hint(tmp_path: Path) -> None:
    with pytest.raises(tuff.TuffError) as raised:
        tuff.Workspace.open(tmp_path / "missing")
    assert raised.value.kind in {"not_found", "usage", "io"}
    with pytest.raises(tuff.TuffError):
        tuff.Pack.build(tmp_path)


def test_checkout_from_git(tmp_path: Path, workspace: tuff.Workspace) -> None:
    import subprocess

    repo = tmp_path / "repo"
    shutil.copytree(EXAMPLE_PACK, repo)
    git = ["git", "-c", "user.name=t", "-c", "user.email=t@example.test", "-C", str(repo)]
    subprocess.run(["git", "init", "-q", str(repo)], check=True)
    subprocess.run([*git, "add", "."], check=True)
    subprocess.run([*git, "commit", "-q", "-m", "release"], check=True)
    subprocess.run([*git, "tag", "v1.0.0"], check=True)

    pack = tuff.Pack.checkout(repo.as_uri(), targets=["claude"])
    assert pack.version == "1.0.0"
    assert pack.inspect()["source"]["kind"] == "git"
    plan = workspace.plan_install(pack, ["claude"])
    workspace.apply(plan)
    lock = json.loads((workspace.root / "tuff.lock").read_text())
    assert "v1.0.0" in json.dumps(lock)
