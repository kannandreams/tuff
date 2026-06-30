from __future__ import annotations

import argparse
import sys
from pathlib import Path

from loadout.codex_adapter import install_codex_skill
from loadout.drift import diff_against_baseline, get_drift_status
from loadout.errors import LoadoutError
from loadout.lockfile import init_lockfile, require_lockfile
from loadout.manifest import load_manifest


def main(argv: list[str] | None = None) -> int:
    parser = _build_parser()
    args = parser.parse_args(argv)

    try:
        return args.handler(args)
    except LoadoutError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="loadout")
    subparsers = parser.add_subparsers(required=True)

    init_parser = subparsers.add_parser("init", help="initialize .loadout state")
    init_parser.set_defaults(handler=_handle_init)

    add_parser = subparsers.add_parser("add", help="install a local primitive")
    add_parser.add_argument("primitive", help="path to a primitive directory")
    add_parser.set_defaults(handler=_handle_add)

    list_parser = subparsers.add_parser("list", help="list installed primitives")
    list_parser.set_defaults(handler=_handle_list)

    diff_parser = subparsers.add_parser("diff", help="diff an installed primitive against baseline")
    diff_parser.add_argument("primitive_id")
    diff_parser.set_defaults(handler=_handle_diff)

    return parser


def _handle_init(_args: argparse.Namespace) -> int:
    lock_path = init_lockfile(Path.cwd())
    print(f"initialized {lock_path.relative_to(Path.cwd())}")
    return 0


def _handle_add(args: argparse.Namespace) -> int:
    repo_root = Path.cwd()
    lockfile = require_lockfile(repo_root)
    manifest = load_manifest((repo_root / args.primitive).resolve())
    target_path = install_codex_skill(repo_root, lockfile, manifest)
    print(f"installed {manifest.id} -> {target_path.relative_to(repo_root)}")
    return 0


def _handle_list(_args: argparse.Namespace) -> int:
    repo_root = Path.cwd()
    lockfile = require_lockfile(repo_root)
    if not lockfile.primitives:
        print("no primitives installed")
        return 0

    for primitive_id, entry in sorted(lockfile.primitives.items()):
        status = get_drift_status(repo_root, entry)
        print(f"{primitive_id}\t{entry.version}\t{status.status}\t{entry.installed_target_path}")
    return 0


def _handle_diff(args: argparse.Namespace) -> int:
    repo_root = Path.cwd()
    lockfile = require_lockfile(repo_root)
    entry = lockfile.primitives.get(args.primitive_id)
    if entry is None:
        raise LoadoutError(f"primitive is not installed: {args.primitive_id}")

    diff = diff_against_baseline(repo_root, entry)
    if diff:
        print(diff, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
