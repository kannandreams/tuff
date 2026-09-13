---
title: Validate in CI
description: Fail a build when installed capabilities drift from the lockfile.
---

## `tuff check`

Validate installed capabilities for CI. Exits 1 on any failure.

```sh frame="terminal"
tuff check                    # check all capabilities
tuff check --global           # check global capabilities only
tuff check --json             # machine-readable JSON output
tuff check --ignore-failures  # report failures but exit 0
```

Example output:

```text
✓ python-uv-default       skill      open-agents  ok
✗ dirty-skill             skill      open-agents  modified (.agents/skills/dirty-skill/SKILL.md)
```

To also check that installed MCP servers actually start, add [`tuff mcp doctor`](/cli/mcp/#tuff-mcp-doctor).

## CI with GitHub Actions

Add this to your project's `.github/workflows/tuff-check.yml`:

```yaml
name: Tuff Check
on: [push, pull_request]

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Build and install tuff
        run: cargo install tuffcli

      - name: Validate capabilities
        run: tuff check --json
```

Commit `tuff.lock` to your repo so
`tuff check` runs against the committed state. See [The tuff.lock File](/concepts/lockfile)
for what to commit.
