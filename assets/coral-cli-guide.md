---
name: coral-cli-guide
description: Reference for using Coral CLI to manage agent capabilities — install, list, diff, check, update, scope, and import operations.
---

# Coral CLI Guide

You have the Coral capability lifecycle manager installed in this project.
Use these commands when the user asks about managing skills, tools, hooks,
or workflows, or when they mention "drift", "baseline", or "coral".

## Available Commands

### Install
- `coral add <path> -t <target>` — install a local capability
- `coral add <git-url> --skill <name> -t <target>` — install from git
- `coral add <git-url> --tool <name> -t <target>` — install tool from git
- `coral add <git-url> --hook <name> -t <target>` — install hook from git
- `coral import <path> -t <target>` — track existing agent files
- `coral import -t <target>` — import all existing capabilities

### Inspect
- `coral list` — show all installed capabilities with drift status
- `coral list --type skill` — filter by capability type
- `coral list --scope global` — show global scope
- `coral status` — detailed status with override warnings
- `coral diff <id>` — show local changes against baseline
- `coral diff <id> --upstream` — show upstream changes (git sources only)
- `coral outdated` — check all capabilities for available updates

### Update & Merge
- `coral update <id> --check` — dry run: preview what would change
- `coral update <id>` — attempt three-way merge with upstream
- `coral update <id> --force` — overwrite local changes with upstream

### CI & Validation
- `coral check` — validate all capabilities (exit 1 on any failure)
- `coral check --json` — machine-readable output for CI

### Manage
- `coral remove <id>` — remove a capability
- `coral target list` — show available harness targets
- `coral target add <id>` — register a target
- `coral init --global` — initialize global scope (~/.coral/)

### Targets
- `open-agents` — Codex, Cursor, OpenCode, Copilot, Gemini CLI, Roo, Cline
- `claude` — Claude Code

### Scope
- Project (default): `.coral/` in repo root — committed with project
- Global: `~/.coral/` in home directory — available across all projects
- Project always wins when both exist

## Directory Model
- `.agents/skills/` — create and edit skills here (single source of truth)
- `.agents/tools/` — create and edit tools here
- `.agents/hooks/` — create and edit hooks here
- `.agents/workflows/` — create and edit workflows here
- `.coral/` — coral state (committed)
- No separate source directory — agent files are the source

## Status Values
- `clean` — installed content matches baseline
- `modified` — local changes detected (run `coral diff <id>` to see)
- `missing` — installed file no longer exists

## Quick Cheat Sheet
```
coral init                          # initialize repo
coral add <path> -t open-agents     # install capability
coral list                          # check drift
coral diff <id>                     # see what changed
coral check                         # CI validation
coral outdated                      # check for updates
coral update <id>                   # update from source
```
