"""Tests for tuff_hooks_example.py against SPEC.md version 0.2.0, organised by
the section 7 conformance checklist item(s) each test covers. Run with
`python3 -m unittest` from this directory.

Item 15 ("Agreement with Tuff") requires running an external conformance
kit against the real `tuff` binary, which this clean-room task explicitly
forbids touching; see test_item15_agreement_with_tuff_is_out_of_scope
below, which documents the skip rather than silently omitting the item.

The fixture spec below is *not* the real hooks-spec.json; it is a small,
self-contained document in the same shape (valid against
hooks-spec.schema.json) built to exercise both settings shapes (grouped
and flat), all three coverage levels, and -- deliberately -- an alias that
collides with another row's canonical name, which the real hooks-spec.json
does not currently exhibit but which section 5 explicitly describes.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

TOY = Path(__file__).resolve().parent / "tuff_hooks_example.py"

sys.path.insert(0, str(Path(__file__).resolve().parent))
import tuff_hooks_example as toy  # noqa: E402


def _toml_str(value: str) -> str:
    """Render `value` as a TOML basic string literal (quotes, backslashes,
    and control characters escaped)."""
    out = ['"']
    for ch in value:
        if ch == "\\":
            out.append("\\\\")
        elif ch == '"':
            out.append('\\"')
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\t":
            out.append("\\t")
        elif ch == "\r":
            out.append("\\r")
        else:
            out.append(ch)
    out.append('"')
    return "".join(out)


def _event(name, blocking="not_blocking"):
    return {
        "event": name,
        "canonical_name": name,
        "blocking": blocking,
        "since_spec_version": "0.1.0",
        "payload_schema": {"fields": []},
    }


def _row(event, native_event, coverage, aliases=None, scope=None, caveat=None):
    return {
        "event": event,
        "native_event": native_event,
        "aliases": aliases or [],
        "coverage": coverage,
        "scope": scope or [],
        "caveat": caveat,
        "source": None,
        "since_harness_version": None,
        "until_harness_version": None,
    }


CANONICAL_EVENTS = [
    "session_start", "session_end", "pre_tool_use", "post_tool_use",
    "before_finish", "after_save", "stop",
]


def make_fixture_spec() -> dict:
    events = [_event(n) for n in CANONICAL_EVENTS]

    grouped_adapter = {
        "adapter": "fixture-grouped",
        "display_name": "Fixture Grouped",
        "dir_prefix": ".fixture",
        "hook_settings_path": ".fixture/settings.json",
        "hook_settings_shape": "grouped",
        "hook_command": "sh .fixture/hooks/<id>/run.sh",
        "spec_version": "0.2.0",
        "events": [
            _row("session_start", "SessionStart", "full", aliases=["SessionStart"]),
            _row("session_end", None, "unsupported", caveat="no session end"),
            _row("pre_tool_use", None, "unsupported", caveat="no pre tool hook"),
            _row("post_tool_use", None, "unsupported", caveat="no post tool hook"),
            # before_finish aliases "stop" -- a deliberate collision with the
            # *canonical* name of another row, to test resolution order.
            _row("before_finish", "BeforeFinish", "partial",
                 aliases=["stop", "AltStop"], scope=["main agent"],
                 caveat="approximate"),
            _row("after_save", None, "unsupported", caveat="no after save"),
            _row("stop", "Stop", "full"),
        ],
    }

    flat_adapter = {
        "adapter": "fixture-flat",
        "display_name": "Fixture Flat",
        "dir_prefix": ".fixtureflat",
        "hook_settings_path": ".fixtureflat/hooks.json",
        "hook_settings_shape": "flat",
        "hook_command": "sh .fixtureflat/hooks/<id>/run.sh",
        "spec_version": "0.2.0",
        "events": [
            _row("session_start", "sessionStart", "full"),
            _row("session_end", "sessionEnd", "full"),
            _row("pre_tool_use", "preToolUse", "full"),
            _row("post_tool_use", "postToolUse", "full"),
            _row("before_finish", "stop", "partial", scope=["agent completion"], caveat="flat caveat"),
            _row("after_save", None, "unsupported", caveat="no after save"),
            _row("stop", "stop", "full"),
        ],
    }

    return {
        "spec_version": "0.2.0",
        "events": events,
        "adapters": [grouped_adapter, flat_adapter],
    }


class ToyTestCase(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="tuff-toy-")
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.spec = make_fixture_spec()
        self.spec_path = Path(self.tmp) / "spec.json"
        self.spec_path.write_text(json.dumps(self.spec), encoding="utf-8")
        self.project = Path(self.tmp) / "project"
        self.project.mkdir()

    def write_manifest(self, subdir, event, command, working_directory=None,
                        hook_id="my-hook", files=None, on_disk_files=None,
                        manifest_type="hook", id_line=None, extra_toml_lines=None):
        """Write a tuff.toml (section 4.1) into `subdir`.

        `files` is the list of strings to put in the manifest's top-level
        `files = [...]` array (what gets *installed*). `on_disk_files` is
        a dict of {relative path: content} to actually create on disk
        beside the manifest (a superset of `files`, to test that
        unlisted files are NOT installed).
        """
        mdir = Path(self.tmp) / subdir
        mdir.mkdir(parents=True, exist_ok=True)
        lines = []
        lines.append(id_line if id_line is not None else f'id = {_toml_str(hook_id)}')
        lines.append(f'type = {_toml_str(manifest_type)}')
        lines.append('version = "1.0.0"')
        lines.append('description = "test hook"')
        if files is not None:
            rendered = ", ".join(_toml_str(f) for f in files)
            lines.append(f'files = [{rendered}]')
        if extra_toml_lines:
            lines.extend(extra_toml_lines)
        lines.append("")
        lines.append("[hook]")
        lines.append(f'event = {_toml_str(event)}')
        lines.append(f'command = {_toml_str(command)}')
        if working_directory is not None:
            lines.append(f'working_directory = {_toml_str(working_directory)}')
        (mdir / "tuff.toml").write_text("\n".join(lines) + "\n", encoding="utf-8")
        for rel, content in (on_disk_files or {}).items():
            p = mdir / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(content, encoding="utf-8")
        return mdir

    def run_toy(self, *args):
        proc = subprocess.run(
            [sys.executable, str(TOY), *args],
            capture_output=True, text=True,
        )
        return proc

    def install(self, harness, manifest_dir, project=None):
        return self.run_toy(
            "install", "--spec", str(self.spec_path),
            "--project", str(project or self.project),
            "--harness", harness, str(manifest_dir),
        )

    def assert_empty_dir(self, d: Path):
        self.assertTrue(d.is_dir())
        self.assertEqual(list(d.iterdir()), [])

    # -- item 1: vocabulary -------------------------------------------------

    def test_item1_recognises_all_seven_canonical_events_by_canonical_name(self):
        """1. Vocabulary: every canonical event name in section 3 is recognised."""
        for name in CANONICAL_EVENTS:
            row = toy.resolve_row(self.spec["adapters"][1], name)  # fixture-flat covers all 7
            self.assertIsNotNone(row, f"canonical event {name!r} was not resolved")
            self.assertEqual(row["event"], name)

    # -- item 2: resolution order (+ refuses a name matching no row) -------

    def test_item2_canonical_name_takes_precedence_over_an_earlier_alias(self):
        """2. Resolution order: a canonical row wins over another row's alias
        of the same spelling."""
        grouped = self.spec["adapters"][0]
        # "stop" is both the canonical name of the `stop` row and an alias of
        # the `before_finish` row. The canonical row MUST win.
        row = toy.resolve_row(grouped, "stop")
        self.assertEqual(row["event"], "stop")
        self.assertEqual(row["native_event"], "Stop")

    def test_item2_aliases_remain_available_when_no_canonical_name_matches(self):
        """2. Resolution order: an alias still resolves when no row's
        canonical name matches the given spelling."""
        grouped = self.spec["adapters"][0]
        row = toy.resolve_row(grouped, "AltStop")
        self.assertIsNotNone(row)
        self.assertEqual(row["event"], "before_finish")

    def test_item2_a_name_matching_no_row_is_refused_naming_supported_events_and_aliases(self):
        """2. Resolution order (section 5): a name that matches no row at
        all is refused like an unsupported event, and the message lists
        supported canonical events with the aliases each accepts."""
        mdir = self.write_manifest("m_no_such_event", "not_a_real_event", "echo hi")
        project = Path(self.tmp) / "proj_no_such_event"
        project.mkdir()
        proc = self.install("fixture-grouped", mdir, project=project)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("not_a_real_event", proc.stderr)
        self.assertIn("session_start", proc.stderr)
        self.assertIn("SessionStart", proc.stderr)  # aliases named too
        self.assert_empty_dir(project)

    # -- item 3: matrix completeness (also exercised via check-matrices) ---

    def test_item3_fixture_matrix_is_complete(self):
        """3. Matrix completeness: the fixture matrices cover every
        canonical event exactly once, name a native event on every
        supported row and on no unsupported row."""
        proc = self.run_toy("check-matrices", "--spec", str(self.spec_path))
        self.assertEqual(proc.returncode, 0, proc.stderr)

    def test_item3_incomplete_matrix_is_reported(self):
        """3. Matrix completeness: a matrix missing a canonical event, or
        naming a native_event on an unsupported row, is flagged."""
        broken = make_fixture_spec()
        broken["adapters"][0]["events"].pop()
        broken["adapters"][1]["events"][4]["coverage"] = "unsupported"
        p = Path(self.tmp) / "broken_spec.json"
        p.write_text(json.dumps(broken), encoding="utf-8")
        proc = self.run_toy("check-matrices", "--spec", str(p))
        self.assertEqual(proc.returncode, 1)
        self.assertIn("must be exactly once", proc.stderr)
        self.assertIn("MUST NOT name a native_event", proc.stderr)

    # -- item 4: honest coverage ---------------------------------------------

    def test_item4_full_coverage_installs_silently(self):
        """4. Honest coverage: a full-coverage event installs with no
        warnings."""
        mdir = self.write_manifest("m1", "session_start", "echo hi")
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        out = json.loads(proc.stdout)
        self.assertEqual(out["warnings"], [])
        self.assertEqual(proc.stderr, "")

    def test_item4_partial_coverage_installs_with_warning(self):
        """4. Honest coverage: a partial-coverage event installs but warns,
        quoting scope and caveat."""
        mdir = self.write_manifest("m2", "before_finish", "echo hi")
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        out = json.loads(proc.stdout)
        self.assertEqual(len(out["warnings"]), 1)
        self.assertIn("main agent", out["warnings"][0])
        self.assertIn("approximate", out["warnings"][0])
        self.assertIn("approximate", proc.stderr)

    def test_item4_unsupported_event_is_refused(self):
        """4. Honest coverage: an unsupported event is refused, with its
        caveat and the events the harness does accept."""
        mdir = self.write_manifest("m3", "session_end", "echo hi")
        project = Path(self.tmp) / "proj_unsupported"
        project.mkdir()
        proc = self.install("fixture-grouped", mdir, project=project)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("session_end", proc.stderr)
        self.assertIn("no session end", proc.stderr)  # the row's caveat
        self.assertIn("session_start", proc.stderr)  # a supported event is named
        self.assert_empty_dir(project)

    # -- item 5: manifest validation ------------------------------------------

    def test_item5_valid_id_is_accepted(self):
        """5. Manifest validation: a plain, possibly nested id is accepted."""
        mdir = self.write_manifest("m5a", "stop", "echo hi", hook_id="security/format-check")
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertTrue((self.project / ".fixture" / "hooks" / "security" / "format-check" / "run.sh").is_file())

    def test_item5_escaping_or_malformed_ids_are_refused(self):
        """5. Manifest validation: an id that is not a relative path of
        plain names is refused, section 4.1's own examples among them."""
        bad_ids = ["../x", "/x", "a//b", "a/", "..", ".", "a\\b", " a", "a "]
        for i, bad_id in enumerate(bad_ids):
            with self.subTest(bad_id=bad_id):
                mdir = self.write_manifest(f"m5b_{i}", "stop", "echo hi", hook_id=bad_id)
                project = Path(self.tmp) / f"proj5b_{i}"
                project.mkdir()
                proc = self.install("fixture-grouped", mdir, project=project)
                self.assertEqual(proc.returncode, 1, f"id {bad_id!r} should have been refused")
                self.assert_empty_dir(project)

    def test_item5_missing_required_manifest_fields_are_refused(self):
        """5. Manifest validation: id/version/description/type are
        required non-empty strings, and event/command must not be empty
        or only whitespace."""
        cases = [
            dict(id_line='type = "hook"'),  # missing id entirely
            dict(manifest_type="not-a-hook"),
            dict(event="   "),
            dict(command=""),
        ]
        for i, kwargs in enumerate(cases):
            with self.subTest(i=i):
                base = dict(event="stop", command="echo hi")
                base.update(kwargs)
                mdir = self.write_manifest(f"m5c_{i}", **base)
                project = Path(self.tmp) / f"proj5c_{i}"
                project.mkdir()
                proc = self.install("fixture-grouped", mdir, project=project)
                self.assertEqual(proc.returncode, 1)
                self.assert_empty_dir(project)

    def test_item5_working_directory_must_be_relative_with_no_dotdot(self):
        """5. Manifest validation: [hook].working_directory must be
        relative with no '..' segment."""
        for bad_wd in ["/abs", "a/../b", ".."]:
            with self.subTest(bad_wd=bad_wd):
                mdir = self.write_manifest("m5d", "stop", "echo hi", working_directory=bad_wd)
                project = Path(self.tmp) / f"proj5d_{bad_wd.replace('/', '_')}"
                project.mkdir()
                proc = self.install("fixture-grouped", mdir, project=project)
                self.assertEqual(proc.returncode, 1)
                self.assert_empty_dir(project)

    def test_item5_files_entries_share_ids_plain_name_rules(self):
        """5. Manifest validation (4.1): a `files` entry is "made of plain
        names in the same sense as id" -- so it shares id's backslash,
        NUL-adjacent, and edge-whitespace restrictions, not just the
        segment-emptiness rule."""
        for bad_entry in ["a\\b.sh", " leading.sh", "trailing.sh "]:
            with self.subTest(bad_entry=bad_entry):
                mdir = self.write_manifest("m5e", "stop", "echo hi", files=[bad_entry])
                project = Path(self.tmp) / f"proj5e_{abs(hash(bad_entry))}"
                project.mkdir()
                proc = self.install("fixture-grouped", mdir, project=project)
                self.assertEqual(proc.returncode, 1, f"files entry {bad_entry!r} should have been refused")
                self.assert_empty_dir(project)

    # -- item 6: contained runtime files ---------------------------------------

    def test_item6_only_listed_files_are_installed(self):
        """6. Contained runtime files: only files named in `files` are
        installed; a file merely present beside the manifest is not."""
        mdir = self.write_manifest(
            "m6a", "stop", "scripts/summarize.sh",
            files=["scripts/summarize.sh"],
            on_disk_files={
                "scripts/summarize.sh": "#!/bin/sh\necho summarized\n",
                "scripts/unlisted.sh": "#!/bin/sh\necho should not be installed\n",
            },
        )
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        out = json.loads(proc.stdout)
        hook_dir = self.project / ".fixture" / "hooks" / "my-hook"
        self.assertIn(".fixture/hooks/my-hook/scripts/summarize.sh", out["files"])
        self.assertTrue((hook_dir / "scripts" / "summarize.sh").is_file())
        self.assertFalse((hook_dir / "scripts" / "unlisted.sh").exists())
        self.assertFalse((hook_dir / "tuff.toml").exists())

    def test_item6_src_prefix_is_stripped_once(self):
        """4.2 / item 6: one leading src/ is stripped from the installed path."""
        mdir = self.write_manifest(
            "m6b", "stop", "echo hi",
            files=["src/check.sh", "src/src/nested.sh"],
            on_disk_files={"src/check.sh": "a", "src/src/nested.sh": "b"},
        )
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        hook_dir = self.project / ".fixture" / "hooks" / "my-hook"
        self.assertTrue((hook_dir / "check.sh").is_file())
        self.assertTrue((hook_dir / "src" / "nested.sh").is_file())  # only ONE leading src/ removed

    def test_item6_a_repeated_files_entry_installs_once(self):
        """6. Contained runtime files (4.2, revised): "An entry listed
        more than once, including once with a leading ./ and once
        without, is installed once" -- not refused."""
        mdir = self.write_manifest(
            "m6f", "stop", "echo hi",
            files=["scripts/a.sh", "scripts/a.sh", "./scripts/a.sh"],
            on_disk_files={"scripts/a.sh": "content"},
        )
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        out = json.loads(proc.stdout)
        self.assertEqual(out["files"].count(".fixture/hooks/my-hook/scripts/a.sh"), 1)
        hook_dir = self.project / ".fixture" / "hooks" / "my-hook"
        self.assertEqual((hook_dir / "scripts" / "a.sh").read_text(), "content")

    def test_item6_two_different_entries_colliding_on_one_installed_path_are_refused(self):
        """6. Contained runtime files (4.2, revised): two *different*
        listed files that would install under the same path (e.g.
        `check.sh` and `src/check.sh`) are refused -- distinct from a
        literal repeat of the same entry, which is not."""
        mdir = self.write_manifest(
            "m6g", "stop", "echo hi",
            files=["check.sh", "src/check.sh"],
            on_disk_files={"check.sh": "one", "src/check.sh": "two"},
        )
        project = Path(self.tmp) / "proj6g"
        project.mkdir()
        proc = self.install("fixture-grouped", mdir, project=project)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("check.sh", proc.stderr)
        self.assert_empty_dir(project)

    def test_item6_a_files_entry_that_climbs_out_is_refused(self):
        """6. Contained runtime files: an entry escaping the manifest
        directory is refused (also covered by 5's plain-names rule)."""
        mdir = self.write_manifest("m6c", "stop", "echo hi", files=["../escape.sh"])
        project = Path(self.tmp) / "proj6c"
        project.mkdir()
        proc = self.install("fixture-grouped", mdir, project=project)
        self.assertEqual(proc.returncode, 1)
        self.assert_empty_dir(project)

    def test_item6_a_symlinked_files_entry_is_refused_not_followed(self):
        """6. Contained runtime files: a listed file that is, or passes
        through, a symbolic link is refused."""
        mdir = self.write_manifest("m6d", "stop", "echo hi", files=["link.sh"])
        outside = Path(self.tmp) / "outside_secret.sh"
        outside.write_text("secret", encoding="utf-8")
        try:
            (mdir / "link.sh").symlink_to(outside)
        except (OSError, NotImplementedError) as exc:
            self.skipTest(f"symlinks unavailable in this environment: {exc}")
        project = Path(self.tmp) / "proj6d"
        project.mkdir()
        proc = self.install("fixture-grouped", mdir, project=project)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("symbolic link", proc.stderr)
        self.assert_empty_dir(project)

    def test_item6_a_listed_file_cannot_replace_the_wrapper(self):
        """6. Contained runtime files: a listed file that would install as
        'run.sh' is refused, since it would replace the wrapper."""
        mdir = self.write_manifest(
            "m6e", "stop", "echo hi", files=["run.sh"],
            on_disk_files={"run.sh": "#!/bin/sh\necho evil\n"},
        )
        project = Path(self.tmp) / "proj6e"
        project.mkdir()
        proc = self.install("fixture-grouped", mdir, project=project)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("run.sh", proc.stderr)
        self.assert_empty_dir(project)

    # -- item 7: refusal changes nothing ---------------------------------------

    def test_item7_a_refused_install_leaves_a_preexisting_project_untouched(self):
        """7. Refusal changes nothing: refusing an install must not alter
        any file the project already had (not just leave PROJECT_DIR
        empty -- it must leave what was already there alone)."""
        settings_path = self.project / ".fixture" / "settings.json"
        settings_path.parent.mkdir(parents=True)
        preexisting = {"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "sh untouched.sh"}]}]}}
        settings_path.write_text(json.dumps(preexisting), encoding="utf-8")
        before = settings_path.read_bytes()

        mdir = self.write_manifest("m7", "session_end", "echo hi")  # unsupported -> refusal
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 1)
        self.assertEqual(settings_path.read_bytes(), before)
        self.assertFalse((self.project / ".fixture" / "hooks" / "my-hook").exists())

    def test_section4_4_a_step4_write_failure_is_an_error_and_reports_what_was_written(self):
        """4.4 (revised): steps 1-3 (validate, resolve, compute the merge)
        must refuse cleanly with nothing written; a failure *while
        writing* in step 4 is instead "an error rather than a refusal",
        and an implementation "SHOULD report which files it had already
        written." This is not one of the 15 numbered checklist items, but
        it is new normative text in 4.4.

        Triggered here without any monkeypatching: two distinct, both
        individually valid, `files` entries -- `src/aaa` (installs as
        `aaa`) and `aaa/ccc` (installs as `aaa/ccc`) -- pass every 4.1/4.2
        validation check (their installed paths differ, so they are not a
        "same path" refusal), but collide on the filesystem once actually
        written: `aaa` is written as a plain file first, then `aaa/ccc`
        needs `aaa` to be a directory.
        """
        mdir = self.write_manifest(
            "m_step4", "stop", "echo hi",
            files=["src/aaa", "aaa/ccc"],
            on_disk_files={"src/aaa": "a-content", "aaa/ccc": "c-content"},
        )
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("aaa", proc.stderr)
        self.assertIn("already written", proc.stderr)
        # Unlike a steps-1-3 refusal, the file(s) written before the
        # failure are allowed to remain -- confirm the wrapper survived.
        self.assertTrue((self.project / ".fixture" / "hooks" / "my-hook" / "run.sh").is_file())

    # -- item 8: no execution -----------------------------------------------

    def test_item8_command_is_never_executed_at_install(self):
        """8. No execution: the hook's command must never run while
        installing."""
        marker = Path(self.tmp) / "marker.txt"
        mdir = self.write_manifest("m8", "stop", f"touch {marker}")
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertFalse(marker.exists(), "install must not execute the hook command")

    def test_item8_runtime_files_are_not_executed_at_install(self):
        """8. No execution: a runtime file must never run while
        installing, even if executable."""
        marker = Path(self.tmp) / "marker2.txt"
        script = f"#!/bin/sh\ntouch {marker}\n"
        mdir = self.write_manifest(
            "m8b", "stop", "echo hi", files=["scripts/run_me.sh"],
            on_disk_files={"scripts/run_me.sh": script},
        )
        (mdir / "scripts" / "run_me.sh").chmod(0o755)
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertFalse(marker.exists())

    # -- item 9: wrapper --------------------------------------------------------

    def test_item9_wrapper_renders_byte_for_byte_with_the_described_quoting(self):
        """9. Wrapper: renders section 4.3's exact content and quoting,
        including the five-character escape for an embedded single quote."""
        mdir = self.write_manifest("m9", "stop", "echo 'hi'", working_directory=".")
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        run_sh = self.project / ".fixture" / "hooks" / "my-hook" / "run.sh"
        text = run_sh.read_text()
        expected = (
            "#!/usr/bin/env bash\n"
            "set -euo pipefail\n"
            "cd -- '.'\n"
            "exec bash -euo pipefail -c 'echo '\"'\"'hi'\"'\"''\n"
        )
        self.assertEqual(text, expected)

    def test_item9_wrapper_preserves_shell_sensitive_values_when_executed(self):
        """9. Wrapper: shell-sensitive characters round-trip correctly.
        Deliberately *executes* the rendered wrapper, as the spec's own
        test does."""
        tricky = "printf '%s' \"it's a \\$HOME test\""
        mdir = self.write_manifest("m9b", "stop", tricky)
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        run_sh = self.project / ".fixture" / "hooks" / "my-hook" / "run.sh"
        result = subprocess.run(["sh", str(run_sh)], capture_output=True, text=True, cwd=self.project)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "it's a $HOME test")

    def test_item9_nul_byte_in_command_is_rejected(self):
        """9. Wrapper: a value containing NUL is refused, nothing
        written."""
        mdir = Path(self.tmp) / "m9c"
        mdir.mkdir(parents=True, exist_ok=True)
        (mdir / "tuff.toml").write_text(
            'id = "my-hook"\n'
            'type = "hook"\n'
            'version = "1.0.0"\n'
            'description = "test hook"\n'
            "\n"
            "[hook]\n"
            'event = "stop"\n'
            'command = "printf x\\u0000y"\n',
            encoding="utf-8",
        )
        project = Path(self.tmp) / "proj_nul"
        project.mkdir()
        proc = self.install("fixture-grouped", mdir, project=project)
        self.assertEqual(proc.returncode, 1)
        self.assertIn("NUL", proc.stderr)
        self.assert_empty_dir(project)

    # -- item 10: fragment validation -------------------------------------------

    def test_item10_grouped_fragment_with_only_hooks_key_is_accepted(self):
        """10. Fragment validation: a grouped fragment with only 'hooks' is
        accepted."""
        frag = {"hooks": {"SessionStart": [{"hooks": [{"type": "command", "command": "sh x"}]}]}}
        fp = Path(self.tmp) / "frag_ok.json"
        fp.write_text(json.dumps(frag), encoding="utf-8")
        proc = self.run_toy("validate-fragment", "--spec", str(self.spec_path), "--harness", "fixture-grouped", str(fp))
        self.assertEqual(proc.returncode, 0, proc.stderr)

    def test_item10_whole_settings_file_is_refused_as_a_fragment(self):
        """10. Fragment validation: a fragment carrying a key beyond
        'hooks' (a whole settings file, e.g. with an unrelated top-level
        key) is refused."""
        frag = {"hooks": {}, "unrelatedTopLevelKey": True}
        fp = Path(self.tmp) / "frag_bad.json"
        fp.write_text(json.dumps(frag), encoding="utf-8")
        proc = self.run_toy("validate-fragment", "--spec", str(self.spec_path), "--harness", "fixture-grouped", str(fp))
        self.assertEqual(proc.returncode, 1)
        self.assertIn("unrelatedTopLevelKey", proc.stderr)

    def test_item10_flat_fragment_version_is_optional(self):
        """10. Fragment validation (0.2.0, section 6.2): a flat fragment
        MAY omit 'version'; one with a stray key beyond hooks/version is
        still refused."""
        frag_no_version = {"hooks": {"preToolUse": [{"command": "sh x"}]}}
        fp = Path(self.tmp) / "frag_flat_no_version.json"
        fp.write_text(json.dumps(frag_no_version), encoding="utf-8")
        proc = self.run_toy("validate-fragment", "--spec", str(self.spec_path), "--harness", "fixture-flat", str(fp))
        self.assertEqual(proc.returncode, 0, proc.stderr)

        frag_with_version = {"version": 1, "hooks": {"preToolUse": [{"command": "sh x"}]}}
        fp2 = Path(self.tmp) / "frag_flat_with_version.json"
        fp2.write_text(json.dumps(frag_with_version), encoding="utf-8")
        proc2 = self.run_toy("validate-fragment", "--spec", str(self.spec_path), "--harness", "fixture-flat", str(fp2))
        self.assertEqual(proc2.returncode, 0, proc2.stderr)

        frag_bad = {"version": 1, "hooks": {}, "extra": 1}
        fp3 = Path(self.tmp) / "frag_flat_bad.json"
        fp3.write_text(json.dumps(frag_bad), encoding="utf-8")
        proc3 = self.run_toy("validate-fragment", "--spec", str(self.spec_path), "--harness", "fixture-flat", str(fp3))
        self.assertEqual(proc3.returncode, 1)

    # -- item 11: idempotent merge ---------------------------------------------

    def test_item11_merging_the_same_fragment_twice_does_not_duplicate_the_hook_grouped(self):
        """11. Idempotent merge (grouped shape): installing the same hook
        twice leaves the settings file byte for byte unchanged after the
        first install."""
        mdir = self.write_manifest("m11a", "stop", "echo hi")
        proc1 = self.install("fixture-grouped", mdir)
        self.assertEqual(proc1.returncode, 0, proc1.stderr)
        settings_path = self.project / ".fixture" / "settings.json"
        bytes_after_first = settings_path.read_bytes()

        proc2 = self.install("fixture-grouped", mdir)
        self.assertEqual(proc2.returncode, 0, proc2.stderr)
        bytes_after_second = settings_path.read_bytes()
        self.assertEqual(bytes_after_first, bytes_after_second)

        data = json.loads(bytes_after_second)
        self.assertEqual(len(data["hooks"]["Stop"]), 1)

    def test_item11_merging_the_same_fragment_twice_does_not_duplicate_the_hook_flat(self):
        """11. Idempotent merge (flat shape): same as above, flat shape."""
        mdir = self.write_manifest("m11b", "stop", "echo hi")
        proc1 = self.install("fixture-flat", mdir)
        self.assertEqual(proc1.returncode, 0, proc1.stderr)
        settings_path = self.project / ".fixtureflat" / "hooks.json"
        bytes_after_first = settings_path.read_bytes()

        proc2 = self.install("fixture-flat", mdir)
        self.assertEqual(proc2.returncode, 0, proc2.stderr)
        bytes_after_second = settings_path.read_bytes()
        self.assertEqual(bytes_after_first, bytes_after_second)

        data = json.loads(bytes_after_second)
        self.assertEqual(len(data["hooks"]["stop"]), 1)
        self.assertIn("version", data)

    def test_item11_merging_keeps_what_the_user_already_had(self):
        """11. Idempotent merge: merging preserves keys and registrations
        the implementation did not write."""
        settings_path = self.project / ".fixture" / "settings.json"
        settings_path.parent.mkdir(parents=True)
        preexisting = {
            "hooks": {
                "Stop": [{"hooks": [{"type": "command", "command": "sh some/other/hook.sh"}]}],
                "SessionStart": [{"hooks": [{"type": "command", "command": "sh untouched.sh"}]}],
            },
            "someUserKey": {"nested": True},
        }
        settings_path.write_text(json.dumps(preexisting), encoding="utf-8")

        mdir = self.write_manifest("m11c", "stop", "echo hi")
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)

        data = json.loads(settings_path.read_text())
        self.assertEqual(data["someUserKey"], {"nested": True})
        self.assertIn({"hooks": [{"type": "command", "command": "sh some/other/hook.sh"}]}, data["hooks"]["Stop"])
        self.assertEqual(data["hooks"]["SessionStart"], [{"hooks": [{"type": "command", "command": "sh untouched.sh"}]}])
        self.assertEqual(len(data["hooks"]["Stop"]), 2)

    def test_item11_a_missing_settings_file_is_treated_as_the_shapes_default(self):
        """11. Idempotent merge (6.3): a missing file is treated as {} in
        the grouped shape and {"version": 1} in the flat shape."""
        mdir = self.write_manifest("m11d", "stop", "echo hi")
        proc = self.install("fixture-flat", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        data = json.loads((self.project / ".fixtureflat" / "hooks.json").read_text())
        self.assertEqual(data["version"], 1)

    def test_item11_a_corrupt_settings_file_is_refused_and_left_unchanged(self):
        """11. Idempotent merge (6.3): a settings file that is not a JSON
        object, or whose hooks/event entry is malformed, is refused as
        corrupt and left unchanged."""
        settings_path = self.project / ".fixture" / "settings.json"
        settings_path.parent.mkdir(parents=True)
        settings_path.write_text('["not", "an", "object"]', encoding="utf-8")
        before = settings_path.read_bytes()

        mdir = self.write_manifest("m11e", "stop", "echo hi")
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 1)
        self.assertEqual(settings_path.read_bytes(), before)

    def test_item11_corruption_check_only_inspects_the_event_being_added_to(self):
        """11. Idempotent merge (6.3, revised): "Events the fragment does
        not add to are not inspected, and are kept exactly as they are,
        whatever they hold." A malformed *unrelated* event (not an array)
        must NOT block installing into a different, well-formed event."""
        settings_path = self.project / ".fixture" / "settings.json"
        settings_path.parent.mkdir(parents=True)
        # "SessionStart" is garbage (a string, not an array); we are
        # about to install into "Stop", which this install never reads.
        preexisting = {"hooks": {"SessionStart": "not-an-array-and-thats-fine"}}
        settings_path.write_text(json.dumps(preexisting), encoding="utf-8")

        mdir = self.write_manifest("m11f", "stop", "echo hi")
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)

        data = json.loads(settings_path.read_text())
        self.assertEqual(data["hooks"]["SessionStart"], "not-an-array-and-thats-fine")  # kept, untouched
        self.assertEqual(len(data["hooks"]["Stop"]), 1)

    def test_item11_corruption_check_still_refuses_the_touched_event_when_malformed(self):
        """11. Idempotent merge (6.3, revised): "So MUST a file in which
        an event the fragment adds to is present and not an array" --
        the event this install *does* touch is still checked."""
        settings_path = self.project / ".fixture" / "settings.json"
        settings_path.parent.mkdir(parents=True)
        preexisting = {"hooks": {"Stop": "not-an-array"}}
        settings_path.write_text(json.dumps(preexisting), encoding="utf-8")
        before = settings_path.read_bytes()

        mdir = self.write_manifest("m11g", "stop", "echo hi")
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 1)
        self.assertEqual(settings_path.read_bytes(), before)

    # -- item 12: records --------------------------------------------------------

    def test_item12_records_path_native_canonical_command_and_hash(self):
        """12. Records: install reports settings path, native event,
        canonical event, command, and an entry hash computed over the
        object carrying the command (the typed entry, not the group)."""
        mdir = self.write_manifest("m12", "before_finish", "echo hi")
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        out = json.loads(proc.stdout)
        self.assertEqual(len(out["registrations"]), 1)
        reg = out["registrations"][0]
        self.assertEqual(reg["settings_path"], ".fixture/settings.json")
        self.assertEqual(reg["native_event"], "BeforeFinish")
        self.assertEqual(reg["canonical_event"], "before_finish")
        self.assertEqual(reg["command"], "sh .fixture/hooks/my-hook/run.sh")

        expected_entry = {"type": "command", "command": "sh .fixture/hooks/my-hook/run.sh"}
        expected_hash = toy.hash_entry(expected_entry)
        self.assertEqual(reg["entry_hash"], expected_hash)
        self.assertEqual(len(reg["entry_hash"]), 64)
        self.assertEqual(reg["entry_hash"], reg["entry_hash"].lower())
        int(reg["entry_hash"], 16)  # must be valid hex

    def test_item12_hash_covers_the_typed_entry_not_the_group_flat_too(self):
        """12. Records: in the flat shape the hashed entry is the entry
        itself."""
        mdir = self.write_manifest("m12b", "stop", "echo hi")
        proc = self.install("fixture-flat", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        out = json.loads(proc.stdout)
        reg = out["registrations"][0]
        expected_hash = toy.hash_entry({"command": reg["command"]})
        self.assertEqual(reg["entry_hash"], expected_hash)

    # -- item 13: surgical removal ------------------------------------------

    def test_item13_removal_takes_out_only_tuff_registrations_and_keeps_a_shared_group(self):
        """13. Surgical removal (grouped shape): remove deletes only its
        own entry, even when it shares a group with a user's own entry,
        leaves the user's other registrations and keys untouched, and
        deletes the installed files."""
        settings_path = self.project / ".fixture" / "settings.json"
        settings_path.parent.mkdir(parents=True)
        preexisting = {
            "hooks": {
                # A group the user added by hand under the same event our
                # hook will register under, holding one entry already.
                "Stop": [{"hooks": [{"type": "command", "command": "sh some/other/hook.sh"}]}],
            },
            "someUserKey": True,
        }
        settings_path.write_text(json.dumps(preexisting), encoding="utf-8")

        mdir = self.write_manifest("m13", "stop", "echo hi", hook_id="removable")
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        out = json.loads(proc.stdout)

        # Simulate a user hand-editing the *same* group our hook occupies
        # to add a second entry (sharing the group, per section 6.5's "A
        # group that held the recorded entry and another entry the user
        # added keeps the user's entry.").
        data = json.loads(settings_path.read_text())
        our_command = out["registrations"][0]["command"]
        for group in data["hooks"]["Stop"]:
            if group["hooks"][0]["command"] == our_command:
                group["hooks"].append({"type": "command", "command": "sh hand-added.sh"})
        settings_path.write_text(json.dumps(data), encoding="utf-8")

        regs_path = Path(self.tmp) / "regs.json"
        regs_path.write_text(json.dumps(out["registrations"]), encoding="utf-8")

        hook_dir = self.project / ".fixture" / "hooks" / "removable"
        self.assertTrue(hook_dir.exists())

        proc2 = self.run_toy(
            "remove", "--spec", str(self.spec_path), "--project", str(self.project),
            "--harness", "fixture-grouped", "--registrations", str(regs_path), "removable",
        )
        self.assertEqual(proc2.returncode, 0, proc2.stderr)

        data = json.loads(settings_path.read_text())
        self.assertEqual(data["someUserKey"], True)
        commands = [e["command"] for group in data["hooks"]["Stop"] for e in group["hooks"]]
        self.assertNotIn(our_command, commands)
        self.assertIn("sh some/other/hook.sh", commands)
        self.assertIn("sh hand-added.sh", commands)
        self.assertFalse(hook_dir.exists())

    def test_item13_removal_prunes_an_event_left_empty(self):
        """13. Surgical removal: when removing empties an event's array,
        the event key itself is pruned."""
        mdir = self.write_manifest("m13b", "stop", "echo hi", hook_id="onlyone")
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        out = json.loads(proc.stdout)
        regs_path = Path(self.tmp) / "regs2.json"
        regs_path.write_text(json.dumps(out["registrations"]), encoding="utf-8")

        proc2 = self.run_toy(
            "remove", "--spec", str(self.spec_path), "--project", str(self.project),
            "--harness", "fixture-grouped", "--registrations", str(regs_path), "onlyone",
        )
        self.assertEqual(proc2.returncode, 0, proc2.stderr)

        settings_path = self.project / ".fixture" / "settings.json"
        data = json.loads(settings_path.read_text())
        self.assertNotIn("Stop", data.get("hooks", {}))

    def test_item13_removal_also_removes_the_hooks_root_when_left_empty(self):
        """13. Surgical removal (6.5): "SHOULD also remove
        <dir_prefix>/hooks/ when it is left empty" -- but not when
        another hook's directory is still there."""
        mdir_a = self.write_manifest("m13c_a", "stop", "echo a", hook_id="hook-a")
        mdir_b = self.write_manifest("m13c_b", "stop", "echo b", hook_id="hook-b")
        proc_a = self.install("fixture-grouped", mdir_a)
        proc_b = self.install("fixture-grouped", mdir_b)
        self.assertEqual(proc_a.returncode, 0, proc_a.stderr)
        self.assertEqual(proc_b.returncode, 0, proc_b.stderr)
        out_a = json.loads(proc_a.stdout)
        out_b = json.loads(proc_b.stdout)

        hooks_root = self.project / ".fixture" / "hooks"

        regs_a = Path(self.tmp) / "regs_a.json"
        regs_a.write_text(json.dumps(out_a["registrations"]), encoding="utf-8")
        self.run_toy("remove", "--spec", str(self.spec_path), "--project", str(self.project),
                     "--harness", "fixture-grouped", "--registrations", str(regs_a), "hook-a")
        self.assertTrue(hooks_root.is_dir())  # hook-b's directory is still there
        self.assertTrue((hooks_root / "hook-b").exists())

        regs_b = Path(self.tmp) / "regs_b.json"
        regs_b.write_text(json.dumps(out_b["registrations"]), encoding="utf-8")
        self.run_toy("remove", "--spec", str(self.spec_path), "--project", str(self.project),
                     "--harness", "fixture-grouped", "--registrations", str(regs_b), "hook-b")
        self.assertFalse(hooks_root.exists())  # now empty, and SHOULD be removed too

    def test_item13_removal_prunes_empty_intermediate_directories_for_a_nested_id(self):
        """13. Surgical removal (6.5, revised): "SHOULD also remove each
        directory left empty between the hook directory and
        <dir_prefix>/hooks/, that one included, which for a nested id
        such as security/format-check includes security/." """
        mdir = self.write_manifest("m13d", "stop", "echo hi", hook_id="security/format-check")
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        out = json.loads(proc.stdout)

        hooks_root = self.project / ".fixture" / "hooks"
        self.assertTrue((hooks_root / "security" / "format-check" / "run.sh").is_file())

        regs_path = Path(self.tmp) / "regs_nested.json"
        regs_path.write_text(json.dumps(out["registrations"]), encoding="utf-8")
        proc2 = self.run_toy(
            "remove", "--spec", str(self.spec_path), "--project", str(self.project),
            "--harness", "fixture-grouped", "--registrations", str(regs_path), "security/format-check",
        )
        self.assertEqual(proc2.returncode, 0, proc2.stderr)

        # Both the hook's own directory AND the now-empty "security/"
        # intermediate directory must be gone, and hooks/ itself too
        # since nothing else is left in it.
        self.assertFalse((hooks_root / "security").exists())
        self.assertFalse(hooks_root.exists())

    def test_item13_removal_prunes_nested_directory_only_up_to_a_still_used_sibling(self):
        """13. Surgical removal (6.5, revised): pruning stops at the first
        non-empty directory -- a sibling nested hook under the same
        intermediate directory keeps that directory alive."""
        mdir_a = self.write_manifest("m13e_a", "stop", "echo a", hook_id="security/format-check")
        mdir_b = self.write_manifest("m13e_b", "stop", "echo b", hook_id="security/other-check")
        proc_a = self.install("fixture-grouped", mdir_a)
        proc_b = self.install("fixture-grouped", mdir_b)
        self.assertEqual(proc_a.returncode, 0, proc_a.stderr)
        self.assertEqual(proc_b.returncode, 0, proc_b.stderr)
        out_a = json.loads(proc_a.stdout)

        hooks_root = self.project / ".fixture" / "hooks"
        regs_a = Path(self.tmp) / "regs_nested_a.json"
        regs_a.write_text(json.dumps(out_a["registrations"]), encoding="utf-8")
        self.run_toy(
            "remove", "--spec", str(self.spec_path), "--project", str(self.project),
            "--harness", "fixture-grouped", "--registrations", str(regs_a), "security/format-check",
        )
        self.assertFalse((hooks_root / "security" / "format-check").exists())
        self.assertTrue((hooks_root / "security" / "other-check").exists())  # sibling survives
        self.assertTrue((hooks_root / "security").is_dir())  # kept alive by the sibling

    def test_item13_removal_stops_on_a_corrupt_settings_file_and_changes_nothing(self):
        """13 / 7 (revised 6.5, 7): "removal stops with the hook's files
        still in place and the hook still recorded" when the settings
        file exists but is not valid JSON -- matching install's "refusal
        changes nothing" guarantee, now extended to removal."""
        mdir = self.write_manifest("m13f", "stop", "echo hi", hook_id="corrupt-removal")
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        out = json.loads(proc.stdout)

        settings_path = self.project / ".fixture" / "settings.json"
        settings_path.write_text("{not valid json", encoding="utf-8")
        before = settings_path.read_bytes()
        hook_dir = self.project / ".fixture" / "hooks" / "corrupt-removal"
        self.assertTrue(hook_dir.is_dir())

        regs_path = Path(self.tmp) / "regs_corrupt.json"
        regs_path.write_text(json.dumps(out["registrations"]), encoding="utf-8")
        proc2 = self.run_toy(
            "remove", "--spec", str(self.spec_path), "--project", str(self.project),
            "--harness", "fixture-grouped", "--registrations", str(regs_path), "corrupt-removal",
        )
        self.assertEqual(proc2.returncode, 1)
        self.assertEqual(settings_path.read_bytes(), before)  # unchanged
        self.assertTrue(hook_dir.is_dir())  # the hook's files are still in place

    def test_item13_removal_of_a_settings_file_with_no_hooks_object_is_a_no_op_not_an_error(self):
        """13 (6.5, revised): "A settings file that does not exist, or
        that has no hooks object, holds nothing to remove" -- valid JSON
        with no (or a malformed) hooks key is not an error, unlike
        invalid JSON."""
        mdir = self.write_manifest("m13g", "stop", "echo hi", hook_id="no-hooks-object")
        proc = self.install("fixture-grouped", mdir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        out = json.loads(proc.stdout)

        settings_path = self.project / ".fixture" / "settings.json"
        settings_path.write_text(json.dumps({"someOtherKey": True}), encoding="utf-8")

        regs_path = Path(self.tmp) / "regs_no_hooks.json"
        regs_path.write_text(json.dumps(out["registrations"]), encoding="utf-8")
        proc2 = self.run_toy(
            "remove", "--spec", str(self.spec_path), "--project", str(self.project),
            "--harness", "fixture-grouped", "--registrations", str(regs_path), "no-hooks-object",
        )
        self.assertEqual(proc2.returncode, 0, proc2.stderr)
        data = json.loads(settings_path.read_text())
        self.assertEqual(data, {"someOtherKey": True})  # left exactly alone
        # the hook directory is still removed even though the settings
        # file had nothing to remove
        self.assertFalse((self.project / ".fixture" / "hooks" / "no-hooks-object").exists())

    # -- item 14: published matrix validates against the schema ------------

    def test_item14_valid_fixture_matrix_passes_check_matrices(self):
        """14. Published matrix: a well-formed document (matching the
        shape of hooks-spec.schema.json) passes check-matrices."""
        proc = self.run_toy("check-matrices", "--spec", str(self.spec_path))
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(proc.stdout.strip(), "ok")

    def test_item14_document_with_unexpected_top_level_key_fails(self):
        """14. Published matrix: a document violating the schema's
        additionalProperties:false at the top level is rejected."""
        broken = make_fixture_spec()
        broken["extra_top_level"] = True
        p = Path(self.tmp) / "broken2.json"
        p.write_text(json.dumps(broken), encoding="utf-8")
        proc = self.run_toy("check-matrices", "--spec", str(p))
        self.assertEqual(proc.returncode, 1)
        self.assertIn("unexpected top-level keys", proc.stderr)

    def test_item14_document_with_bad_semver_fails(self):
        """14. Published matrix: spec_version must match the semver
        pattern from the schema."""
        broken = make_fixture_spec()
        broken["spec_version"] = "not-a-semver"
        p = Path(self.tmp) / "broken3.json"
        p.write_text(json.dumps(broken), encoding="utf-8")
        proc = self.run_toy("check-matrices", "--spec", str(p))
        self.assertEqual(proc.returncode, 1)
        self.assertIn("semver", proc.stderr)

    # -- item 15: agreement with Tuff (out of scope) ----------------------------

    @unittest.skip(
        "Item 15 requires running the conformance kit's compare.py against "
        "the real `tuff` binary and comparing byte-for-byte output. This "
        "clean-room task is expressly forbidden from running any `tuff` "
        "binary or reading anything under the real Tuff checkout, so this "
        "item cannot be exercised here and is skipped rather than faked."
    )
    def test_item15_agreement_with_tuff_is_out_of_scope(self):
        """15. Agreement with Tuff: not testable under this task's
        clean-room constraints. See the skip reason."""
        raise AssertionError("should never run")


if __name__ == "__main__":
    unittest.main()
