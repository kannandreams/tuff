#!/usr/bin/env python3
"""Compare a hooks-spec implementation against the real `tuff` binary.

Two groups of cases run against every harness in the specification
document.

Event cases install the same Tuff-standard hook into two fresh projects,
one with `tuff add` and one with the implementation under test, once for
every canonical event, alias, and native name the harness's matrix
mentions, plus one name no row knows. They compare the outcome, the hook
directory byte for byte, the settings file as JSON, the recorded
registrations and their hashes, the partial-coverage warning, a second
install, and removal.

Rule cases each exercise one rule the specification states and a first
implementation is likely to get wrong: a repeated `files` entry, two
entries colliding on one installed path, a nested id and the directories
its removal prunes, a corrupt settings file at install and at removal, and
a malformed event the install does not touch.

Usage:
    compare.py --tuff PATH --impl PATH --spec PATH [--keep]

The implementation is driven through the command-line contract documented
in README.md beside this file. Exit status is 0 when every case agrees and
1 otherwise, with each disagreement printed.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

HOOK_ID = "conformance-hook"
COMMAND = "echo 'it''s here' && sh scripts/check.sh"
WORKING_DIRECTORY = "sub dir"
RUNTIME_FILES = {"scripts/check.sh": "#!/bin/sh\necho checked\n"}
UNLISTED_FILES = {"notes.txt": "not listed in files, never installed\n"}


@dataclass
class Fixture:
    """One manifest and the settings file the user already had."""

    event: str
    hook_id: str = HOOK_ID
    files: list[str] = field(default_factory=lambda: list(RUNTIME_FILES))
    contents: dict[str, str] = field(default_factory=lambda: {**RUNTIME_FILES, **UNLISTED_FILES})
    settings: str | None = None  # None means the default user settings for the shape


@dataclass
class Result:
    ok: bool
    stderr: str
    hook_files: dict[str, bytes] = field(default_factory=dict)
    settings: object = None
    settings_bytes: bytes | None = None
    registrations: list[dict] = field(default_factory=list)
    hooks_tree: list[str] = field(default_factory=list)


def run(cmd: list[str], cwd: Path, env: dict[str, str]) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True)


def write_manifest(root: Path, fixture: Fixture) -> Path:
    manifest_dir = root / "source" / "capability"
    if manifest_dir.exists():
        shutil.rmtree(manifest_dir)
    for rel, content in fixture.contents.items():
        path = manifest_dir / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
    files = ", ".join(json.dumps(rel) for rel in fixture.files)
    command = COMMAND.replace("\\", "\\\\").replace('"', '\\"')
    (manifest_dir / "tuff.toml").write_text(
        f'id = "{fixture.hook_id}"\n'
        'type = "hook"\n'
        'version = "1.0.0"\n'
        'description = "Conformance fixture."\n'
        f"files = [{files}]\n\n"
        "[hook]\n"
        f'event = "{fixture.event}"\n'
        f'command = "{command}"\n'
        f'working_directory = "{WORKING_DIRECTORY}"\n'
    )
    return manifest_dir


def user_settings(adapter: dict) -> str:
    """A settings file the user already had, in the harness's shape."""
    if adapter["hook_settings_shape"] == "flat":
        return json.dumps(
            {"version": 1, "hooks": {"stop": [{"command": "users-own-hook"}]}, "userKey": True},
            indent=2,
        )
    return json.dumps(
        {
            "userKey": True,
            "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "users-own-hook"}]}]},
        },
        indent=2,
    )


def snapshot(project: Path, adapter: dict, hook_id: str) -> tuple[dict[str, bytes], object, bytes | None, list[str]]:
    hooks_root = project / adapter["dir_prefix"] / "hooks"
    hook_dir = hooks_root / hook_id
    files = {}
    if hook_dir.is_dir():
        for path in sorted(hook_dir.rglob("*")):
            if path.is_file():
                files[path.relative_to(hook_dir).as_posix()] = path.read_bytes()
    tree = []
    if hooks_root.is_dir():
        tree = sorted(path.relative_to(hooks_root.parent).as_posix() for path in [hooks_root, *hooks_root.rglob("*")] if path.is_dir())
    settings_path = project / adapter["hook_settings_path"]
    raw = settings_path.read_bytes() if settings_path.is_file() else None
    try:
        parsed = json.loads(raw) if raw else None
    except json.JSONDecodeError:
        parsed = "<not JSON>"
    return files, parsed, raw, tree


def tuff_project(tuff: str, root: Path, adapter: dict, fixture: Fixture, env: dict[str, str]) -> Path:
    project = root / "tuff-project"
    project.mkdir(parents=True)
    run([tuff, "init"], project, env).check_returncode()
    if adapter["adapter"] != "open-agents":
        run([tuff, "harness", "add", adapter["adapter"]], project, env).check_returncode()
    settings = project / adapter["hook_settings_path"]
    settings.parent.mkdir(parents=True, exist_ok=True)
    settings.write_text(fixture.settings if fixture.settings is not None else user_settings(adapter))
    return project


def impl_project(root: Path, adapter: dict, fixture: Fixture) -> Path:
    project = root / "impl-project"
    settings = project / adapter["hook_settings_path"]
    settings.parent.mkdir(parents=True, exist_ok=True)
    settings.write_text(fixture.settings if fixture.settings is not None else user_settings(adapter))
    return project


def tuff_install(tuff: str, project: Path, manifest: Path, adapter: dict, hook_id: str, env) -> Result:
    proc = run([tuff, "add", str(manifest), "-a", adapter["adapter"]], project, env)
    files, parsed, raw, tree = snapshot(project, adapter, hook_id)
    registrations = []
    if proc.returncode == 0:
        lock = json.loads((project / "tuff.lock").read_text())
        for row in lock["capabilities"]:
            if row["name"] == hook_id and row["target"] == adapter["adapter"]:
                for hook in row.get("managed_hooks", []):
                    registrations.append(
                        {
                            "settings_path": hook["settingsPath"],
                            "native_event": hook["event"],
                            "canonical_event": hook.get("canonicalEvent"),
                            "command": hook["command"],
                            "entry_hash": hook["baselineHash"],
                        }
                    )
    return Result(proc.returncode == 0, proc.stderr, files, parsed, raw, registrations, tree)


def impl_install(impl: str, spec: str, project: Path, manifest: Path, adapter: dict, hook_id: str, env) -> Result:
    proc = run(
        [sys.executable, impl, "install", "--spec", spec, "--project", str(project),
         "--harness", adapter["adapter"], str(manifest)],
        project, env,
    )
    files, parsed, raw, tree = snapshot(project, adapter, hook_id)
    registrations = []
    if proc.returncode == 0:
        registrations = json.loads(proc.stdout)["registrations"]
    return Result(proc.returncode == 0, proc.stderr, files, parsed, raw, registrations, tree)


def tuff_remove(tuff: str, project: Path, adapter: dict, hook_id: str, env) -> bool:
    proc = run([tuff, "delete", hook_id, "-a", adapter["adapter"], "--force"], project, env)
    return proc.returncode == 0


def impl_remove(args, root: Path, project: Path, adapter: dict, hook_id: str, registrations: list[dict], env) -> bool:
    registrations_file = root / "registrations.json"
    registrations_file.write_text(json.dumps(registrations))
    proc = run(
        [sys.executable, args.impl, "remove", "--spec", args.spec, "--project", str(project),
         "--harness", adapter["adapter"], "--registrations", str(registrations_file), hook_id],
        project, env,
    )
    return proc.returncode == 0


def last_line(text: str) -> list[str]:
    return text.strip().splitlines()[-1:]


def compare_install(label: str, adapter: dict, fixture: Fixture, t: Result, i: Result, problems: list[str]) -> bool:
    """Compare two installs. Returns whether both succeeded."""
    if t.ok != i.ok:
        problems.append(
            f"{label}: tuff {'installed' if t.ok else 'refused'}, "
            f"implementation {'installed' if i.ok else 'refused'}\n"
            f"    tuff stderr: {last_line(t.stderr)}\n"
            f"    impl stderr: {last_line(i.stderr)}"
        )
        return False
    if not t.ok:
        original = (fixture.settings if fixture.settings is not None else user_settings(adapter)).encode()
        if i.settings_bytes != original:
            problems.append(f"{label}: implementation changed the settings file on refusal")
        if i.hook_files:
            problems.append(f"{label}: implementation wrote files on refusal: {sorted(i.hook_files)}")
        return False
    if sorted(t.hook_files) != sorted(i.hook_files):
        problems.append(
            f"{label}: installed files differ\n    tuff: {sorted(t.hook_files)}\n    impl: {sorted(i.hook_files)}"
        )
    for rel in sorted(set(t.hook_files) & set(i.hook_files)):
        if t.hook_files[rel] != i.hook_files[rel]:
            problems.append(
                f"{label}: {rel} differs\n    tuff: {t.hook_files[rel]!r}\n    impl: {i.hook_files[rel]!r}"
            )
    if t.settings != i.settings:
        problems.append(
            f"{label}: settings differ as JSON\n    tuff: {json.dumps(t.settings)}\n    impl: {json.dumps(i.settings)}"
        )
    strip = lambda rows: [{k: v for k, v in r.items() if k != "entry_hash"} for r in rows]
    if strip(t.registrations) != strip(i.registrations):
        problems.append(
            f"{label}: registrations differ\n    tuff: {strip(t.registrations)}\n    impl: {strip(i.registrations)}"
        )
    elif [r["entry_hash"] for r in t.registrations] != [r["entry_hash"] for r in i.registrations]:
        problems.append(
            f"{label}: entry hashes differ (tuff {[r['entry_hash'][:12] for r in t.registrations]}, "
            f"impl {[r['entry_hash'][:12] for r in i.registrations]})"
        )
    t_warned = "partial compatibility" in t.stderr
    i_warned = "partial" in i.stderr.lower()
    if t_warned != i_warned:
        problems.append(f"{label}: partial-coverage warning: tuff {t_warned}, implementation {i_warned}")
    return True


class Case:
    """Two fresh projects for one harness and one fixture."""

    def __init__(self, args, adapter: dict, fixture: Fixture):
        self.args = args
        self.adapter = adapter
        self.fixture = fixture
        self.root = Path(tempfile.mkdtemp(prefix="hooks-conformance-"))
        self.env = {**os.environ, "HOME": str(self.root / "home"), "NO_COLOR": "1"}
        self.manifest = write_manifest(self.root, fixture)
        self.tp = tuff_project(args.tuff, self.root, adapter, fixture, self.env)
        self.ip = impl_project(self.root, adapter, fixture)

    def install(self) -> tuple[Result, Result]:
        hook_id = self.fixture.hook_id
        return (
            tuff_install(self.args.tuff, self.tp, self.manifest, self.adapter, hook_id, self.env),
            impl_install(self.args.impl, self.args.spec, self.ip, self.manifest, self.adapter, hook_id, self.env),
        )

    def remove(self, registrations: list[dict]) -> tuple[bool, bool]:
        hook_id = self.fixture.hook_id
        return (
            tuff_remove(self.args.tuff, self.tp, self.adapter, hook_id, self.env),
            impl_remove(self.args, self.root, self.ip, self.adapter, hook_id, registrations, self.env),
        )

    def snapshots(self):
        hook_id = self.fixture.hook_id
        return snapshot(self.tp, self.adapter, hook_id), snapshot(self.ip, self.adapter, hook_id)

    def close(self):
        if self.args.keep:
            print(f"kept {self.root}", file=sys.stderr)
        else:
            shutil.rmtree(self.root, ignore_errors=True)


def event_case(args, adapter: dict, event: str) -> list[str]:
    problems: list[str] = []
    label = f"{adapter['adapter']} / {event}"
    case = Case(args, adapter, Fixture(event=event))
    try:
        t, i = case.install()
        if not compare_install(label, adapter, case.fixture, t, i, problems):
            return problems

        t2, i2 = case.install()
        if t2.settings_bytes != t.settings_bytes:
            problems.append(f"{label}: tuff's second install changed its settings file")
        if i2.settings_bytes != i.settings_bytes:
            problems.append(f"{label}: implementation's second install changed its settings file")

        case.remove(i.registrations)
        (tf, ts, _, _), (ifs, is_, _, _) = case.snapshots()
        if ts != is_:
            problems.append(
                f"{label}: settings after removal differ\n    tuff: {json.dumps(ts)}\n    impl: {json.dumps(is_)}"
            )
        if tf or ifs:
            problems.append(f"{label}: hook files left after removal: tuff {sorted(tf)}, impl {sorted(ifs)}")
        return problems
    finally:
        case.close()


def rule_cases(args, adapter: dict) -> tuple[int, list[str]]:
    """Cases for rules a first implementation is likely to get wrong."""
    event = next(row["event"] for row in adapter["events"] if row["coverage"] != "unsupported")
    name = adapter["adapter"]
    problems: list[str] = []
    count = 0

    def install_only(label: str, fixture: Fixture) -> None:
        nonlocal count
        count += 1
        case = Case(args, adapter, fixture)
        try:
            t, i = case.install()
            compare_install(label, adapter, fixture, t, i, problems)
        finally:
            case.close()

    # 4.2: a repeated entry installs once; colliding entries are refused.
    install_only(
        f"{name} / repeated files entry",
        Fixture(event=event, files=["scripts/check.sh", "./scripts/check.sh"]),
    )
    install_only(
        f"{name} / files colliding on one installed path",
        Fixture(
            event=event,
            files=["check.sh", "src/check.sh"],
            contents={"check.sh": "echo top\n", "src/check.sh": "echo src\n"},
        ),
    )

    # 6.3: a corrupt file is refused; an event the install does not touch is kept as it is.
    install_only(f"{name} / settings file not JSON", Fixture(event=event, settings="{ not json"))
    install_only(
        f"{name} / untouched event is not an array",
        Fixture(event=event, settings=json.dumps({"hooks": {"UnrelatedEvent": "not an array"}, "userKey": True})),
    )

    # 6.5: a nested id, and the empty directories its removal prunes.
    count += 1
    label = f"{name} / nested id installed and removed"
    case = Case(args, adapter, Fixture(event=event, hook_id=f"security/{HOOK_ID}"))
    try:
        t, i = case.install()
        if compare_install(label, adapter, case.fixture, t, i, problems):
            case.remove(i.registrations)
            (tf, ts, _, ttree), (ifs, is_, _, itree) = case.snapshots()
            if ts != is_:
                problems.append(f"{label}: settings after removal differ\n    tuff: {json.dumps(ts)}\n    impl: {json.dumps(is_)}")
            if ttree != itree:
                problems.append(f"{label}: directories left under hooks/ differ\n    tuff: {ttree}\n    impl: {itree}")
    finally:
        case.close()

    # 6.5: removal stops on a corrupt settings file with every file still in place.
    count += 1
    label = f"{name} / removal with a corrupt settings file"
    case = Case(args, adapter, Fixture(event=event))
    try:
        t, i = case.install()
        if compare_install(label, adapter, case.fixture, t, i, problems):
            for project in (case.tp, case.ip):
                (project / adapter["hook_settings_path"]).write_text("{ not json")
            t_ok, i_ok = case.remove(i.registrations)
            if t_ok != i_ok:
                problems.append(f"{label}: tuff removal {'succeeded' if t_ok else 'failed'}, implementation {'succeeded' if i_ok else 'failed'}")
            (tf, _, _, _), (ifs, _, _, _) = case.snapshots()
            if sorted(tf) != sorted(ifs):
                problems.append(f"{label}: files left differ\n    tuff: {sorted(tf)}\n    impl: {sorted(ifs)}")
    finally:
        case.close()

    return count, problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--tuff", required=True)
    parser.add_argument("--impl", required=True)
    parser.add_argument("--spec", required=True)
    parser.add_argument("--keep", action="store_true", help="keep the temporary projects")
    args = parser.parse_args()
    # Each implementation runs with its temporary project as the working
    # directory, so every path handed to it must be absolute.
    args.tuff = str(Path(args.tuff).resolve())
    args.impl = str(Path(args.impl).resolve())
    args.spec = str(Path(args.spec).resolve())

    spec = json.loads(Path(args.spec).read_text())
    canonical = [event["event"] for event in spec["events"]]
    cases = 0
    problems: list[str] = []
    for adapter in spec["adapters"]:
        names = list(canonical)
        # Every alias and native name the matrix mentions, plus one name no row knows.
        for row in adapter["events"]:
            names.extend(row["aliases"])
            if row["native_event"]:
                names.append(row["native_event"])
        names.append("no_such_event")
        for event in dict.fromkeys(names):
            cases += 1
            problems.extend(event_case(args, adapter, event))
        count, rule_problems = rule_cases(args, adapter)
        cases += count
        problems.extend(rule_problems)

    for problem in problems:
        print(problem)
    print(f"\n{cases} cases, {len(problems)} disagreements")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
