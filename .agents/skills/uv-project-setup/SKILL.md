---
name: uv-project-setup
version: 1.0.0
description: Set up a Python project with uv in a consistent, modern, and reproducible way.
triggers:
  - "set up python project"
  - "uv setup"
  - "init project with uv"
  - "install uv project"
  - "create python project"
allowed-tools: [Read, Bash]
---

# uv Project Setup Skill

## When to invoke this skill

Set up a Python project with `uv` in a consistent, modern, and reproducible
way.

Do NOT use this for Docker setup, CI configuration, or non-Python projects.
This skill is specifically for initializing a Python project with the `uv`
package manager — environment, dependencies, and basic layout only.

## Inputs

- project name
- Python version
- package layout preference
- dependency requirements
- testing needs

## Outputs

- project initialization steps
- dependency management approach
- basic project layout
- recommended developer commands
- verification commands

## Rules

- Prefer reproducible dependency management.
- Keep setup minimal and explicit.
- Align the structure with the project's actual needs.
- Avoid adding tools that do not solve a current problem.

## Recommended Standards

- Use `pyproject.toml` as the package configuration entry point.
- Commit `uv.lock` for deterministic environments when the project is application-oriented.
- Use a `src/` layout for import isolation in packaged Python projects.
- Keep runtime and development dependencies explicit.
- Run commands through `uv run`.

## Preferred Commands

- `uv venv`
- `uv sync`
- `uv add <package>`
- `uv add --dev <package>`
- `uv run pytest`

## Example Workflow

1. Confirm the project name, Python version, and package layout preference.
2. Initialize the project with `uv init` or create `pyproject.toml` by hand.
3. Add core dependencies with `uv add` and dev dependencies with `uv add --dev`.
4. Set up `src/` layout if the project is packaged, or flat layout for simple applications.
5. Verify the setup with `uv run pytest` and document the main developer commands.

## Acceptance Criteria

The setup is acceptable when it:

- uses `uv` for environment and dependency management
- has a clear package layout
- documents the main developer commands
- states whether `uv.lock` is committed and why
- avoids unnecessary tooling complexity
