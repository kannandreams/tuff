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
manifest file. Tuff discovers the skill's structure automatically. For
source-based development, a `tuff.toml` in the source directory is optional
but not emitted into agent directories.

A skill can say what version it is. Tuff reads `version` from `tuff.toml` when there is one, otherwise `version:` or `metadata.version:` from the `SKILL.md` frontmatter, which is where the [Agent Skills specification](https://agentskills.io/specification) puts it. That declared version becomes the installed version and shows in `tuff list`. For a git source it is marked `(declared)`, since a tag such as `v1.2.0` is the stronger claim; see [Install a release](/cli#install-a-release).

## Installing a skill

```sh frame="terminal"
# Local directory
tuff add --agent open-agents ./my-skill

# Git repository
tuff add skill https://github.com/owner/repo <name> --agent open-agents

# Global scope
tuff add --agent open-agents --global ./my-skill
```

Tuff tracks where each skill came from, records a baseline, and reports local drift after
installation. See the [CLI reference](/cli) for available lifecycle commands.
