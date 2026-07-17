---
name: just-command-orchestration
version: 1.0.0
description: Design a Justfile that centralizes project command wrappers and execution orchestration without hiding business logic.
triggers:
  - "add justfile"
  - "just command wrappers"
  - "command orchestration"
  - "project commands"
  - "task runner"
  - "justfile"
allowed-tools: [Read, Write, Bash]
---

# Just Command Orchestration Skill

## When to invoke this skill

Design or maintain a `Justfile` as the single project entry point for common
developer, CI, release, and operations commands.

Use this when a project has repeated shell commands, multi-step local workflows,
or inconsistent command names across docs, CI, and developer machines.

Do NOT use this to move application logic into `just`. Recipes should wrap
stable commands and orchestrate execution. Business logic belongs in source
code, scripts with tests, or purpose-built tools.

## Inputs

- project language and toolchain
- existing commands from README, CI, scripts, and package files
- commands that mutate state or touch external systems
- required environment variables
- local vs CI execution differences
- preferred command naming conventions

## Outputs

- `Justfile` command surface
- recipe grouping and naming plan
- default command and help output
- safety boundaries for destructive or external commands
- documentation updates for canonical command usage
- verification checklist for command wrappers

## Rules

- Keep `just` recipes thin: compose commands, do not encode domain logic.
- Use stable names such as `setup`, `check`, `test`, `lint`, `format`,
  `build`, `run`, `release`, and `clean`.
- Make `just` the documented command entry point for humans and agents.
- Keep mutating commands explicit; require a clear recipe name and visible
  arguments for deploy, publish, delete, reset, and migration actions.
- Prefer recipes that call existing package-manager, test-runner, or tool
  commands rather than duplicating their flags in multiple places.
- Use `set dotenv-load := true` only when the project intentionally relies on
  local `.env` files.
- Avoid recipes that depend on hidden shell session state.

## Example Workflow

1. Inventory commands from README, CI, package config, scripts, and Makefiles.
2. Group commands by intent: setup, verification, development, build, release,
   operations.
3. Choose canonical recipe names and aliases only when they reduce confusion.
4. Move multi-step command sequences into recipes with explicit parameters.
5. Add a default recipe that lists available commands.
6. Update docs and CI to call the `just` recipes where appropriate.
7. Verify every recipe from a clean shell.

## Recipe Shape

```make
set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    just --list

setup:
    uv sync

check: lint test

lint:
    uv run ruff check .

test:
    uv run pytest
```

## Acceptance Criteria

The command layer is acceptable when it:

- names one canonical command entry point for common project actions
- keeps recipes small enough to inspect in one pass
- separates read-only checks from mutating or external actions
- documents required environment variables and external tools
- verifies the important recipes from a clean shell
- removes duplicated command sequences from docs where practical
