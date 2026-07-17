---
title: Workflows
description: Workflows compose skills, tools, and hooks into named operational patterns.
---

A workflow capability declares a reusable pattern. It lists which skills, tools, and hooks
should be installed together to support a specific workflow (e.g., "test-first Rust development").

Workflows don't execute anything. They are declarative bundles that surface a checklist
of required capabilities. The harness (Codex, Claude, etc.) reads the workflow definition
and activates the listed capabilities together.

## Manifest

```toml
# coral.toml
id = "release-prep"
version = "1.0.0"
type = "workflow"
description = "Pre-release checks and validation steps before cutting a release."

[[workflow.requires]]
id = "python-uv-default"
type = "skill"

[[workflow.requires]]
id = "security-review"
type = "tool"

[[workflow.requires]]
id = "pre-commit-lint"
type = "hook"
```

| Field | Required | Description |
|---|---|---|
| `id` | Yes | Stable identifier |
| `version` | Yes | Semantic version |
| `type` | Yes | Must be `"workflow"` |
| `description` | Yes | What this workflow enables |
| `[[workflow.requires]]` | Yes | At least one required capability |

Each `[[workflow.requires]]` entry must have:

| Field | Required | Description |
|---|---|---|
| `id` | Yes | The capability to depend on. Cannot be the workflow's own id. |
| `type` | Yes | `"skill"`, `"tool"`, or `"hook"` |

## Installing a workflow

```sh frame="terminal"
$ coral add ./release-prep -a open-agents
note: workflow 'release-prep' requires 3 capabilities:
  - python-uv-default (skill)
  - security-review (tool)
  - pre-commit-lint (hook)
installed release-prep (open-agents) -> .agents/workflows/release-prep/workflow.toml
```

Workflows install themselves but do **not** auto-install dependencies. Install the required
capabilities separately, or use `coral add <path> -a <agent>` if they already exist in your
project.

## Validation

At install time, Coral validates:

- `requires` has at least one entry
- No duplicate requirement ids
- No self-reference (workflow cannot require itself)
- All requirement ids are non-empty

## Where files go

| Target | Workflow directory |
|---|---|
| `open-agents` | `.agents/workflows/<id>/workflow.toml` |
| `claude` | `.claude/workflows/<id>/workflow.toml` |

## `coral status` for workflows

Shows the workflow and each required capability with its drift status:

```
release-prep     project  clean
  ├─ python-uv-default      skill         clean
  ├─ security-review        tool          modified
  └─ pre-commit-lint        hook          missing
```

Capabilities that aren't installed show `missing`. Installed capabilities show
their actual drift status (`clean`, `modified`).

## Lifecycle

Workflows participate in the full Coral lifecycle: `coral list`, `coral diff`,
`coral check`, `coral delete`, `coral untrack`, and `coral outdated` all work.

## Example: feature-build workflow

```sh frame="terminal"
# 1. Create the workflow
mkdir feature-build
cat > feature-build/coral.toml << 'EOF'
id = "feature-build"
version = "1.0.0"
type = "workflow"
description = "Full feature development cycle with testing, linting, and security review."

[[workflow.requires]]
id = "rust-implement"
type = "skill"

[[workflow.requires]]
id = "rust-write-tests"
type = "skill"

[[workflow.requires]]
id = "pre-commit-lint"
type = "hook"

[[workflow.requires]]
id = "security-review"
type = "tool"
EOF

# 2. Install the workflow
coral add ./feature-build -a open-agents

# 3. Install the required capabilities
coral add https://github.com/pproenca/dot-skills --skill rust-implement -a open-agents
coral add https://github.com/pproenca/dot-skills --skill rust-write-tests -a open-agents
coral add examples/hooks/pre-commit-lint -a open-agents
coral add examples/tools/security-review -a open-agents

# 4. Check status
coral status
```
