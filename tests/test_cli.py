from pathlib import Path

from loadout.cli import main


def make_primitive(tmp_path: Path) -> Path:
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
    (primitive / "src" / "SKILL.md").write_text("# Example\n\nOriginal text.\n")
    return primitive


def test_cli_lifecycle(tmp_path: Path, monkeypatch, capsys) -> None:
    primitive = make_primitive(tmp_path)
    monkeypatch.chdir(tmp_path)

    assert main(["init"]) == 0
    assert (tmp_path / ".loadout" / "lock.json").exists()

    assert main(["add", str(primitive)]) == 0
    assert main(["list"]) == 0
    output = capsys.readouterr().out
    assert "example\t1.0.0\tclean\t.agents/skills/example/SKILL.md" in output

    installed = tmp_path / ".agents" / "skills" / "example" / "SKILL.md"
    installed.write_text("# Example\n\nChanged text.\n")

    assert main(["list"]) == 0
    assert "example\t1.0.0\tmodified\t.agents/skills/example/SKILL.md" in capsys.readouterr().out

    assert main(["diff", "example"]) == 0
    diff_output = capsys.readouterr().out
    assert "-Original text." in diff_output
    assert "+Changed text." in diff_output


def test_cli_add_requires_init(tmp_path: Path, monkeypatch, capsys) -> None:
    primitive = make_primitive(tmp_path)
    monkeypatch.chdir(tmp_path)

    assert main(["add", str(primitive)]) == 1
    assert "run 'loadout init' first" in capsys.readouterr().err
