---
title: Use Cases Overview
description: How Coral supports common workflows, from first-time setup to CI validation.
---

## Coral workflow

<div class="lifecycle-flow" aria-label="Coral capability workflow">
  <div class="lifecycle-node lifecycle-node--neutral">
    <strong>Record a baseline</strong>
    <span><code>coral create</code> or <code>coral add</code> brings a capability under management.</span>
  </div>
  <div class="lifecycle-branches">
    <section class="lifecycle-track lifecycle-track--drift" aria-label="Local changes">
      <div class="lifecycle-track-label">Local changes</div>
      <div class="lifecycle-step">Files change in <code>.agents/</code> or <code>.claude/</code>.</div>
      <div class="lifecycle-node lifecycle-node--drift">
        <strong>Review drift</strong>
        <span><code>coral list</code> and <code>coral diff</code> compare the files with the recorded baseline.</span>
      </div>
    </section>
    <section class="lifecycle-track lifecycle-track--upstream" aria-label="Upstream changes">
      <div class="lifecycle-track-label">Upstream changes</div>
      <div class="lifecycle-step">A git-backed repository moves ahead.</div>
      <div class="lifecycle-node lifecycle-node--upstream">
        <strong>Review an update</strong>
        <span><code>coral outdated</code>, <code>coral diff --upstream</code>, and <code>coral update</code> reconcile the change.</span>
      </div>
    </section>
  </div>
  <div class="lifecycle-node lifecycle-node--resolve">
    <strong>Record the reviewed state</strong>
    <span>Accept local edits or merge git-backed changes with <code>coral update</code>.</span>
  </div>
</div>

## Use Case: First-time setup

Setting up Coral in a new or existing project.

```sh frame="terminal"
# Install coral
curl -fsSL https://raw.githubusercontent.com/kannandreams/coral/main/install.sh | sh

# Initialize the project
cd my-project
coral init
# → creates coral.lock, registers open-agents, and scaffolds .agents/ directories
# → auto-installs coral-cli-guide; your agent now knows Coral commands

# Register agent harnesses
coral agent add open-agents
coral agent add claude

# Verify
coral list
# coral-cli-guide    0.1.0    project    open-agents    clean
```

## Use Case: Create and manage a local capability

Author a skill, track it, edit it, and manage drift, all in one directory.

```sh frame="terminal"
# 1. Scaffold a skill
coral create skill python-project

# 2. Coral has already recorded the baseline and lockfile entry
# created and tracked python-project (open-agents)

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
coral update python-project

# 8. Untrack the capability while keeping its files
coral untrack python-project -a open-agents
```

Use `coral update` when the local edited version is now the source of truth and you want Coral to
record a new baseline in place. For git-sourced capabilities, the same command reconciles local
and upstream changes. For capabilities added from an external local folder, it reloads from the
recorded source path.

## Use Case: Install and update from git

Install capabilities from shared repos, check for updates, and merge changes.

```sh frame="terminal"
# 1. Install a skill from a git repository
coral add skill https://github.com/pproenca/dot-skills rust-implement --agent open-agents
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

```sh frame="terminal"
# Repo already has:
#   .agents/skills/existing-skill/SKILL.md
#   .agents/tools/scan-tool/index.js
#   .claude/skills/claude-only/SKILL.md

# 1. Initialize Coral
coral init
# existing .agents/ directories stay untouched
# .agents/workflows/ created (new: Coral introduces this)

# 2. Register your agents
coral agent add open-agents
coral agent add claude

# 3. Add the existing directories in place
coral add --agent open-agents .agents/skills/existing-skill
coral add --agent open-agents .agents/tools/scan-tool
coral add --agent claude .claude/skills/claude-only
# added existing-skill (skill, open-agents) -> .agents/skills/existing-skill
# added scan-tool (tool, open-agents) -> .agents/tools/scan-tool
# added claude-only (skill, claude) -> .claude/skills/claude-only

# 4. Verify: everything tracked, zero files moved
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
| `.agents/skills/` | Skill runtime files | `coral init` scaffolds, you author |
| `.agents/tools/` | Tool runtime files | Same |
| `.agents/hooks/` | Hook runtime files | Same |
| `.agents/workflows/` | Workflow runtime files | Same |
| `coral.lock` | Committed capability identity and lifecycle metadata | `coral init` |
| `.claude/skills/` etc. | Claude-specific capabilities | `coral add` or you author |

**One directory, one source of truth.** Agent files live in `.agents/` or `.claude/`.
Coral tracks them in place, with no copies or duplication.
