---
name: python-uv-default
version: 0.1.0
description: Use uv for Python dependency and environment management.
---

# Python uv Default

Use `uv` for Python dependency and environment management.

## Rules

- Prefer `uv sync` to install project dependencies.
- Use `uv add` and `uv add --dev` to change dependencies.
- Run project commands through `uv run`.
- Do not use bare `pip install`, Poetry, or Pipenv unless the repository has
  already standardized on that tool.
