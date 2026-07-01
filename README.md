<p align="center">
  <img src="assets/loadout.png" alt="Loadout logo" width="320">
</p>

# Loadout

Loadout is a minimal CLI for installing repo-local agent primitives, tracking
their installed baseline, detecting local edits, and showing diffs against that
baseline.

This first implementation targets Codex-style skills installed at
`.agents/skills/<id>/SKILL.md`.

Loadout core is intentionally content-agnostic. It is the engine for primitive
schema, install state, drift detection, diffing, validation, and future
adapters. Actual skills, tools, hooks, and workflows should live in separate
pack repos or in the user project that owns them.

Think of the split like dbt:

| Layer | Analogy | Contents |
| --- | --- | --- |
| Loadout core | `dbt-core` | CLI, schema, lockfile, diff/merge, validation, adapters |
| Loadout packs | `dbt-utils` or starter packages | Optional shared primitives and curated defaults |
| User projects | dbt projects | Installed and locally modified `.loadout/` primitives |

## Documentation

The documentation site is built with Docusaurus:

```sh
npm ci
npm run docs:start
```

Build the static docs site:

```sh
npm run docs:build
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

The `examples/fixtures/python-uv-default` primitive is a demo/test fixture, not
a bundled standard pack. Production primitives should live in separate pack
repositories or in the repositories that use them.

## License

Loadout is released under the MIT License. See [LICENSE](LICENSE).
