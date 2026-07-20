---
title: Skills
description: Skills teach an agent how to work inside a project.
---

A skill is project-specific instruction. It tells an agent how to work in a repository,
what conventions matter, what workflows are expected, and what domain context it should keep in mind.

Skills are installed into agent-specific directories along with any associated files (scripts,
references, assets):

```text
# Open Agents standard (codex, cursor, opencode, copilot, etc.)
.agents/skills/<id>/SKILL.md

# Claude Code
.claude/skills/<id>/SKILL.md
```

## Manifest

Skills installed from a local directory or git repository do not require a
manifest file. Coral discovers the skill's structure automatically. For
source-based development, a `coral.toml` in the source directory is optional
but not emitted into agent directories.

## Installing a skill

```sh frame="terminal"
# Local directory
coral add --agent open-agents ./my-skill

# Git repository
coral add --agent open-agents skill https://github.com/owner/repo <name>

# Global scope
coral add --agent open-agents --global ./my-skill
```

Coral tracks where each skill came from, records a baseline, and reports local drift after
installation. See the [CLI reference](/cli) for available lifecycle commands.
