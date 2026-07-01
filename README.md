<p align="center">
  <img src="assets/loadout.png" alt="Loadout logo" width="320">
</p>

# Loadout

Loadout is a minimal CLI for installing repo-local agent primitives, tracking
their installed baseline, detecting local edits, and showing diffs against that
baseline.

This first implementation targets Codex-style skills installed at
`.agents/skills/<id>/SKILL.md`.

## Documentation

The documentation site is built with Docusaurus:

```sh
pnpm install
pnpm docs:start
```

Build the static docs site:

```sh
pnpm docs:build
```

## Commands

```sh
just setup
just check
just run -- init
just run -- add examples/fixtures/python-uv-default
just run -- list
just run -- diff python-uv-default
```

The project uses `uv` for dependency management and commits `uv.lock` for
reproducible CLI/test behavior.

## License

Loadout is released under the MIT License. See [LICENSE](LICENSE).
