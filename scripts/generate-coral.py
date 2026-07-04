#!/usr/bin/env python3
"""Generate a half-block "coral" wordmark and write it to assets/coral.txt.

Each ▀ (U+2580) upper-half-block character is intended to be rendered with
background = top pixel color and foreground = bottom pixel color, giving
each character cell a 2-color vertical gradient.

The output is plain text (no embedded ANSI codes) — coloring is applied at
render time by the CLI so the gradient can adapt to the terminal.
"""

import os

# --- Letter definitions (8 tall x 6 wide dot grids, 1=dot, 0=empty) ---

LETTERS: dict[str, list[list[int]]] = {
    "c": [
        [0, 1, 1, 1, 1, 0],
        [1, 1, 0, 0, 0, 0],
        [1, 1, 0, 0, 0, 0],
        [1, 1, 0, 0, 0, 0],
        [1, 1, 0, 0, 0, 0],
        [1, 1, 0, 0, 0, 0],
        [0, 1, 1, 0, 0, 0],
        [0, 0, 1, 1, 1, 0],
    ],
    "o": [
        [0, 1, 1, 1, 1, 0],
        [1, 1, 0, 0, 1, 1],
        [1, 1, 0, 0, 1, 1],
        [1, 1, 0, 0, 1, 1],
        [1, 1, 0, 0, 1, 1],
        [1, 1, 0, 0, 1, 1],
        [0, 1, 1, 1, 1, 0],
        [0, 0, 0, 0, 0, 0],
    ],
    "r": [
        [1, 1, 0, 1, 1, 0],
        [1, 1, 1, 0, 0, 1],
        [1, 1, 0, 0, 0, 1],
        [1, 1, 0, 0, 0, 0],
        [1, 1, 0, 0, 0, 0],
        [1, 1, 0, 0, 0, 0],
        [1, 1, 0, 0, 0, 0],
        [1, 1, 0, 0, 0, 0],
    ],
    "a": [
        [0, 1, 1, 1, 1, 0],
        [1, 1, 0, 0, 1, 1],
        [1, 1, 0, 0, 1, 1],
        [0, 0, 0, 0, 1, 1],
        [0, 1, 1, 1, 1, 1],
        [1, 1, 0, 0, 1, 1],
        [1, 1, 0, 0, 1, 1],
        [0, 1, 1, 1, 1, 1],
    ],
    "l": [
        [1, 1, 0, 0, 0, 0],
        [1, 1, 0, 0, 0, 0],
        [1, 1, 0, 0, 0, 0],
        [1, 1, 0, 0, 0, 0],
        [1, 1, 0, 0, 0, 0],
        [1, 1, 0, 0, 0, 0],
        [1, 1, 0, 0, 0, 0],
        [1, 1, 1, 1, 1, 0],
    ],
}

LETTER_WIDTH = 6
LETTER_HEIGHT = 8

# Per-row rightward shift that leans the glyph into an italic slant.
ITALIC_SHIFTS = [2, 2, 1, 1, 1, 0, 0, 0]
MAX_SHIFT = max(ITALIC_SHIFTS)
SLANTED_WIDTH = LETTER_WIDTH + MAX_SHIFT  # 8 columns per slanted letter

# Each half-block row covers 2 vertical pixels (8 / 2 = 4 rows).
HALFBLOCK_ROWS = LETTER_HEIGHT // 2


def italicize(grid: list[list[int]]) -> list[list[int]]:
    """Shear a letter grid so its top leans right, mimicking italic type."""
    slanted = [[0] * SLANTED_WIDTH for _ in range(LETTER_HEIGHT)]
    for row, shift in enumerate(ITALIC_SHIFTS):
        for col in range(LETTER_WIDTH):
            if grid[row][col]:
                slanted[row][col + shift] = 1
    return slanted


def grid_to_halfblocks(grid: list[list[int]]) -> list[str]:
    """Convert an 8-tall dot grid to block characters.

    Each cell pairs two vertical pixels. The output character encodes which
    halves are on so the CLI can apply the right ANSI colours:

      ▀ (U+2580) = top pixel on (or both on) → bg + fg gradient
      ▄ (U+2584) = bottom pixel on           → fg-only, bg transparent
      space      = both pixels off
    """
    width = len(grid[0])
    rows: list[str] = []
    for br in range(HALFBLOCK_ROWS):
        top_row = br * 2
        bottom_row = br * 2 + 1
        line_chars: list[str] = []
        for col in range(width):
            top = grid[top_row][col]
            bottom = grid[bottom_row][col]
            if top:
                line_chars.append("\u2580")  # UPPER HALF BLOCK (top or both)
            elif bottom:
                line_chars.append("\u2584")  # LOWER HALF BLOCK (bottom only)
            else:
                line_chars.append(" ")
        rows.append("".join(line_chars))
    return rows


def build_banner(word: str) -> list[str]:
    """Build a multi-line half-block banner for the given word."""
    letter_grids = [grid_to_halfblocks(italicize(LETTERS[ch])) for ch in word]

    all_lines: list[list[str]] = [[] for _ in range(HALFBLOCK_ROWS)]
    for i, grids in enumerate(letter_grids):
        if i > 0:
            for line in all_lines:
                line.append(" ")
        for r in range(HALFBLOCK_ROWS):
            all_lines[r].append(grids[r])

    return ["".join(line) for line in all_lines]


def main() -> None:
    script_dir = os.path.dirname(os.path.abspath(__file__))
    repo_root = os.path.dirname(script_dir)
    output_path = os.path.join(repo_root, "assets", "coral.txt")

    lines = build_banner("coral")

    with open(output_path, "w", encoding="utf-8") as f:
        for line in lines:
            f.write(line + "\n")

    print(f"Wrote coral banner ({len(lines)} lines) to {output_path}")


if __name__ == "__main__":
    main()
