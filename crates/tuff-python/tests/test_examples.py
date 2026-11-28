"""The Python examples on the site run against this module: the landing
page's example is executed, and every block on the guide page compiles and
names only what the module has."""

import ast
import os
import re
import shutil
from pathlib import Path

import tuff_sdk as tuff

ROOT = Path(__file__).resolve().parents[3]
LANDING = ROOT / "website/src/data/sdk/install.py"
GUIDE = ROOT / "website/src/content/docs/sdk/python.md"
EXAMPLE_PACK = ROOT / "spec/packs/example/review-pack"


def test_landing_example_runs(tmp_path: Path, capsys) -> None:
    tuff.Workspace.init(tmp_path / "my-project")
    shutil.copytree(EXAMPLE_PACK, tmp_path / "review-pack")
    cwd = os.getcwd()
    os.chdir(tmp_path)
    try:
        exec(compile(LANDING.read_text(), str(LANDING), "exec"), {})
    finally:
        os.chdir(cwd)
    assert "create .claude/skills/review-skill/SKILL.md" in capsys.readouterr().out
    assert (tmp_path / "my-project/.claude/skills/review-skill/SKILL.md").is_file()


def names_used(tree: ast.AST) -> set[str]:
    """`tuff.X` and `tuff.X.y` chains in a block."""
    used = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Attribute):
            chain = []
            current: ast.AST = node
            while isinstance(current, ast.Attribute):
                chain.append(current.attr)
                current = current.value
            if isinstance(current, ast.Name) and current.id == "tuff":
                used.add(".".join(reversed(chain)))
    return used


def test_guide_examples_name_real_things() -> None:
    blocks = re.findall(r"```python\n(.*?)```", GUIDE.read_text(), re.S)
    assert len(blocks) >= 4, len(blocks)
    for block in blocks:
        tree = ast.parse(block)
        for name in names_used(tree):
            target = tuff
            for part in name.split("."):
                assert hasattr(target, part), f"tuff_sdk has no {name}:\n{block}"
                target = getattr(target, part)
