---
title: Development Lifecycle
description: How Coral fits into your daily workflow — from first-time setup to CI validation.
---

## Guided flow

<div class="lifecycle-flow">
  <div class="lifecycle-node lifecycle-node--neutral">
    <strong>Capability enters the repo</strong>
    <span>`coral create`, `coral import`, or `coral add` records the first baseline</span>
  </div>
  <div class="lifecycle-branches">
    <div class="lifecycle-track lifecycle-track--drift">
      <div class="lifecycle-track-label">Local edits</div>
      <div class="lifecycle-step">File changes in `.agents/` or `.claude/`</div>
      <div class="lifecycle-node lifecycle-node--drift">
        <strong>Drift detected</strong>
        <span>`coral list` and `coral diff` compare against the recorded baseline</span>
      </div>
    </div>
    <div class="lifecycle-track lifecycle-track--upstream">
      <div class="lifecycle-track-label">Git source updates</div>
      <div class="lifecycle-step">Upstream repository moves ahead</div>
      <div class="lifecycle-node lifecycle-node--upstream">
        <strong>Update reviewed</strong>
        <span>`coral outdated`, `coral diff --upstream`, then `coral update`</span>
      </div>
    </div>
  </div>
  <div class="lifecycle-node lifecycle-node--resolve">
    <strong>Reviewed state recorded</strong>
    <span>Local edits can be accepted with `coral import --override`; git-backed changes can be merged with `coral update`</span>
  </div>
</div>

## Use Case: First-time setup

Setting up Coral in a new or existing project.

```sh
# Install coral
curl -fsSL https://raw.githubusercontent.com/kannandreams/coral/main/install.sh | sh

# Initialize the project
cd my-project
coral init
# → creates .coral/ state, scaffolds .agents/ directories
# → auto-installs coral-cli-guide — your agent now knows coral commands

# Register harness targets
coral target add open-agents
coral target add claude

# Verify
coral list
# coral-cli-guide    0.1.0    project    open-agents    clean
```

## Use Case: Create and manage a local capability

Author a skill, track it, edit it, and manage drift — all in one directory.

```sh
# 1. Scaffold a skill
coral create --skill python-project

# 2. Track it — records baseline + lockfile entry
coral import .agents/skills/python-project -t open-agents
# imported python-project (skill, open-agents)

# 3. Your agent can now read it
coral list
# coral-cli-guide        0.1.0    project    open-agents    clean
# python-project         0.1.0    project    open-agents    clean

# 4. Edit the skill
echo "\nAlways run tests before pushing." >> .agents/skills/python-project/SKILL.md

# 5. Check drift
coral list
# python-project         0.1.0    project    open-agents    modified

# 6. See what changed
coral diff python-project

# 7. Accept changes (update baseline)
coral import .agents/skills/python-project -t open-agents --override
```

Use `--override` when the local edited version is now the source of truth and you want Coral to
record a new baseline in place. `coral update` is the separate flow for git-sourced capabilities
that need reconciliation with upstream.

## Use Case: Install and update from git

Install capabilities from shared repos, check for updates, and merge changes.

```sh
# 1. Install a skill from a git repository
coral add https://github.com/pproenca/dot-skills --skill rust-implement -t open-agents
# installed rust-implement (open-agents) → .agents/skills/rust-implement/SKILL.md

# 2. Check installed capabilities
coral list
# rust-implement         abc1234    project    open-agents    clean

# 3. Check for upstream updates
coral outdated
# rust-implement         skill    open-agents    abc1234    def5678    outdated

# 4. Preview what changed upstream
coral diff rust-implement --upstream
# shows baseline vs latest git source diff

# 5. Dry-run the update
coral update rust-implement --check
# 'rust-implement' can be updated cleanly

# 6. Apply the update (three-way merge)
coral update rust-implement
# installed rust-implement (open-agents) → .agents/skills/rust-implement/SKILL.md

# 7. After editing, check drift
echo "# my custom rule" >> .agents/skills/rust-implement/SKILL.md
coral list
# rust-implement         def5678    project    open-agents    modified
```

## Use Case: Adopt Coral in an existing project

Your repo already has agent files. Bring them under Coral management without moving anything.

```sh
# Repo already has:
#   .agents/skills/existing-skill/SKILL.md
#   .agents/tools/scan-tool/index.js
#   .claude/skills/claude-only/SKILL.md

# 1. Initialize Coral
coral init
# existing .agents/ directories stay untouched
# .agents/workflows/ created (new — Coral introduces this)

# 2. Register your targets
coral target add open-agents
coral target add claude

# 3. Import everything in one command
coral import -t open-agents -t claude
# imported existing-skill (skill, open-agents)
# imported scan-tool (tool, open-agents)
# imported claude-only (skill, claude)

# 4. Verify — everything tracked, zero files moved
coral list
# coral-cli-guide        0.1.0    project    open-agents    clean
# existing-skill         0.1.0    project    open-agents    clean
# scan-tool              0.1.0    project    open-agents    clean
# claude-only            0.1.0    project    claude         clean

# 5. CI is ready
coral check
# ✓ coral-cli-guide        skill    open-agents    ok
# ✓ existing-skill         skill    open-agents    ok
# ✓ scan-tool              tool     open-agents    ok
# ✓ claude-only            skill    claude         ok
```

## Directory model

| Directory | Purpose | Created by |
|---|---|---|
| `.agents/skills/` | Skill source files and coral.toml | `coral init` scaffolds, you author |
| `.agents/tools/` | Tool source files and coral.toml | Same |
| `.agents/hooks/` | Hook source files and coral.toml | Same |
| `.agents/workflows/` | Workflow source files and coral.toml | Same |
| `.coral/` | State — lockfile, baselines, config | `coral init` |
| `.claude/skills/` etc. | Claude-specific capabilities | `coral add` or you author |

**One directory, one source of truth.** Agent files live in `.agents/` or `.claude/`.
Coral tracks them in-place — no copies, no duplication.
