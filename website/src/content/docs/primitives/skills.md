---
title: Skills
description: Skills teach an agent how to work inside a project.
---

A skill is project-specific instruction. It tells an agent how to work in a repository,
what conventions matter, what workflows are expected, and what domain context it should keep in mind.

Skills are installed into target-specific directories along with any associated files (scripts,
references, assets):

```text
# Open Agents standard (codex, cursor, opencode, copilot, etc.)
.agents/skills/<id>/SKILL.md

# Claude Code
.claude/skills/<id>/SKILL.md
```

## Manifest

```toml
# coral.toml
id = "python-uv-default"
version = "0.1.0"
primitive = "skill"
description = "Use uv for Python dependency and environment management."
files = ["src/SKILL.md"]
```

The `files` field lists source files to copy. Each file is written to the target directory
preserving relative paths. For local coral-shaped directories, the `src/` prefix is stripped
during emit.

## Installing a skill

```sh frame="terminal"
# Local directory
coral add ./my-skill -t open-agents

# Git repository
coral add https://github.com/owner/repo --skill <name> -t claude -t open-agents

# Global scope
coral add ./my-skill -t open-agents --global
```

Coral tracks where each skill came from, records a baseline, and reports local drift after
installation. See the [CLI reference](/cli) for available lifecycle commands.
