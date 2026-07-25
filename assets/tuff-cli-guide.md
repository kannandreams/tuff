---
name: tuff-cli-guide
description: Reference for using Tuff CLI to manage agent capabilities — create, add, list, diff, check, update, and scope operations.
---

# Tuff CLI Guide

You have the Tuff capability lifecycle manager installed in this project.
Use these commands when the user asks about managing skills, tools, hooks,
or workflows, or when they mention "drift", "baseline", or "tuff".

## Available Commands

### Install
- `tuff add <path> -a <agent>` — install a local capability (auto-detect type)
- `tuff add skill <path> [name] -a <agent>` — install a skill
- `tuff add tool <path> [name] -a <agent>` — install a tool
- `tuff add hook <path> [name] -a <agent>` — install a hook
- `tuff add workflow <path> [name] -a <agent>` — install a workflow
- `tuff add .agents/skills/<id> -a open-agents` — track existing agent files in place
- `tuff add <git-url> skill <name> -a <agent>` — install skill from git
- `tuff add <git-url> tool <name> -a <agent>` — install tool from git
- `tuff add <git-url> hook <name> -a <agent>` — install hook from git
- `tuff create <type> <id> -a <agent>` — create and track a capability

### Inspect
- `tuff list` — show all installed capabilities with drift status
- `tuff list --type skill` — filter by capability type
- `tuff list --scope global` — show global scope
- `tuff status` — detailed status with override warnings
- `tuff diff <id>` — show local changes against baseline
- `tuff diff <id> --upstream` — show upstream changes (git sources only)
- `tuff outdated` — check all capabilities for available updates

### Update & Merge
- `tuff update <id> --check` — preview local baseline promotion or upstream changes
- `tuff update <id>` — accept local edits or reconcile with upstream
- `tuff update <id> -a <agent>` — update one recorded agent
- `tuff update <id> --force` — overwrite local changes with source output

### CI & Validation
- `tuff check` — validate all capabilities (exit 1 on any failure)
- `tuff check --json` — machine-readable output for CI

### Manage
- `tuff remove <id>` — remove a capability
- `tuff agent list` — show available agent harnesses
- `tuff agent add <id>` — register an agent and initialize its project directory
- `tuff init --global` — initialize global scope

### Agents
- `open-agents` — Codex, Cursor, OpenCode, Copilot, Gemini CLI, Roo, Cline
- `claude` — Claude Code

### Scope
- Project (default): `tuff.lock` in repo root — committed with project
- Global state: XDG config/state/cache Tuff directories — available across all projects
- Project always wins when both exist

## Directory Model
- `.agents/skills/` — create and edit skills here (single source of truth)
- `.agents/tools/` — create and edit tools here
- `.agents/hooks/` — create and edit hooks here
- `.agents/workflows/` — create and edit workflows here
- `tuff.lock` — committed capability identity and lifecycle metadata
- `tuff.config.json` — project preferences; `tuff.lock` remains the project source of truth
- No separate source directory — agent files are the source

## Status Values
- `clean` — installed content matches baseline
- `modified` — local changes detected (run `tuff diff <id>` to see)
- `missing` — installed file no longer exists

## Quick Cheat Sheet
```
tuff init                              # initialize repo and register open-agents
tuff add <path> -a open-agents         # install capability (auto-detect type)
tuff add skill <path> [name]           # install a skill explicitly
tuff list                          # check drift
tuff diff <id>                     # see what changed
tuff check                         # CI validation
tuff outdated                      # check for updates
tuff update <id>                   # accept local edits or update from source
```
