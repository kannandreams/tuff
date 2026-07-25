#!/usr/bin/env python3
"""Small stdlib-only text analyzer for Tuff tool examples."""

from __future__ import annotations

import argparse
from pathlib import Path


def analyze(path: Path, include_preview: bool) -> None:
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    words = text.split()

    print(f"path={path}")
    print(f"lines={len(lines)}")
    print(f"words={len(words)}")
    print(f"characters={len(text)}")

    if include_preview:
        preview = " ".join(words[:20])
        print(f"preview={preview}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Analyze a text file.")
    parser.add_argument("path", help="Text file to analyze")
    parser.add_argument(
        "--preview",
        action="store_true",
        help="Print a short content preview",
    )
    args = parser.parse_args()

    analyze(Path(args.path), args.preview)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
