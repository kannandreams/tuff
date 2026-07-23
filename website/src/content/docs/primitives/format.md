---
title: Capability Format
description: How Coral discovers and tracks capabilities.
---

Coral discovers capabilities from directory structure and, optionally, a source
`coral.toml` manifest. The manifest is **not required** — Coral can infer
type, files, and metadata from the filesystem or from `--type` and `--name`
flags passed at install time.

Tracking metadata lives exclusively in `coral.lock` and
`.coral/objects/`. No tracking files are emitted into agent directories
(`.agents/`, `.claude/`). Coral regenerates derived artifacts like
`CAPABILITIES.md` on demand with `coral generate index`.

## Discovery and auto-detection

Coral infers capability type at `coral add` time from:

1. The `--type` flag (explicit): `--type skill`, `--type tool`, etc.
2. Parent directory name: `skills/` or `skill/` → skill, `tools/` or `tool/` → tool
3. For git repositories: the cloned directory structure

When installing from a git URL, `--name` identifies the capability folder
inside the repository. Coral searches both plural and singular directory
names (`skills/<name>`, `skill/<name>`, tools/<name>`, `tool/<name>`) plus
the repo root.

## Installed output

For a skill with `id = "python-uv-default"`, Coral installs:

```text
.agents/skills/python-uv-default/SKILL.md
```

It records an install-time materialized-tree hash in `coral.lock`; the disposable
verified tree cache is machine-global:

```text
~/.coral/cache/sha256/a1/b2c3...
```

Capabilities tracked from existing project files (e.g., `scripts/deploy.sh`)
are tracked in-place without copying. The lockfile records their source path:

```jsonc
[[capabilities]]
name = "prod-deploy"
type = "tool"
source = "local"
source_path = "scripts/deploy.sh"
sha256 = "..."
target = "open-agents"
installed_path = ".agents/tools/prod-deploy"
```

## Where capabilities should live

Coral core stays content-agnostic. Capabilities live in:

- the user project that owns the capability (any path in the repo)
- an external pack repository maintained by a person, team, or company

Runnable capability examples live under `examples/<type>/`, such as
`examples/skills/` and `examples/tools/`. Test-only inputs belong under
`tests/fixtures/`.
