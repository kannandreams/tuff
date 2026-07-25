---
title: Use Cases Overview
description: How Tuff supports common workflows, from first-time setup to CI validation.
---

## Tuff workflow

<div class="lifecycle-flow" aria-label="Tuff capability workflow">
  <div class="lifecycle-node lifecycle-node--neutral">
    <strong>Record a baseline</strong>
    <span><code>tuff create</code> or <code>tuff add</code> brings a capability under management.</span>
  </div>
  <div class="lifecycle-branches">
    <section class="lifecycle-track lifecycle-track--drift" aria-label="Local changes">
      <div class="lifecycle-track-label">Local changes</div>
      <div class="lifecycle-step">Files change in <code>.agents/</code> or <code>.claude/</code>.</div>
      <div class="lifecycle-node lifecycle-node--drift">
        <strong>Review drift</strong>
        <span><code>tuff list</code> and <code>tuff diff</code> compare the files with the recorded baseline.</span>
      </div>
    </section>
    <section class="lifecycle-track lifecycle-track--upstream" aria-label="Upstream changes">
      <div class="lifecycle-track-label">Upstream changes</div>
      <div class="lifecycle-step">A git-backed repository moves ahead.</div>
      <div class="lifecycle-node lifecycle-node--upstream">
        <strong>Review an update</strong>
        <span><code>tuff outdated</code>, <code>tuff diff --upstream</code>, and <code>tuff update</code> reconcile the change.</span>
      </div>
    </section>
  </div>
  <div class="lifecycle-node lifecycle-node--resolve">
    <strong>Record the reviewed state</strong>
    <span>Accept local edits or merge git-backed changes with <code>tuff update</code>.</span>
  </div>
</div>

## Use Case: First-time setup

Setting up Tuff in a new or existing project.

```sh frame="terminal"
# Install tuff
curl -fsSL https://raw.githubusercontent.com/kannandreams/tuff/main/install.sh | sh

# Initialize the project
cd my-project
tuff init
# → creates tuff.lock, registers open-agents, and scaffolds .agents/ directories
# → auto-installs tuff-cli-guide; your agent now knows Tuff commands

# Register agent harnesses
tuff agent add open-agents
tuff agent add claude

# Verify
tuff list
# tuff-cli-guide    0.1.0    project    open-agents    clean
```

## Use Case: Create and manage a local capability

Author a skill, track it, edit it, and manage drift, all in one directory.

```sh frame="terminal"
# 1. Scaffold a skill
tuff create skill python-project

# 2. Tuff has already recorded the baseline and lockfile entry
# created and tracked python-project (open-agents)

# 3. Your agent can now read it
tuff list
# tuff-cli-guide        0.1.0    project    open-agents    clean
# python-project         0.1.0    project    open-agents    clean

# 4. Edit the skill
echo "\nAlways run tests before pushing." >> .agents/skills/python-project/SKILL.md

# 5. Check drift
tuff list
# python-project         0.1.0    project    open-agents    modified

# 6. See what changed
tuff diff python-project

# 7. Accept changes (update baseline)
tuff update python-project

# 8. Untrack the capability while keeping its files
tuff untrack python-project -a open-agents
```

Use `tuff update` when the local edited version is now the source of truth and you want Tuff to
record a new baseline in place. For git-sourced capabilities, the same command reconciles local
and upstream changes. For capabilities added from an external local folder, it reloads from the
recorded source path.

## Use Case: Install and update from git

Install capabilities from shared repos, check for updates, and merge changes.

```sh frame="terminal"
# 1. Install a skill from a git repository
tuff add skill https://github.com/pproenca/dot-skills rust-implement --agent open-agents
# installed rust-implement (open-agents) → .agents/skills/rust-implement/SKILL.md

# 2. Check installed capabilities
tuff list
# rust-implement         abc1234    project    open-agents    clean

# 3. Check for upstream updates
tuff outdated
# rust-implement         skill    open-agents    abc1234    def5678    outdated

# 4. Preview what changed upstream
tuff diff rust-implement --upstream
# shows baseline vs latest git source diff

# 5. Dry-run the update
tuff update rust-implement --check
# 'rust-implement' can be updated cleanly

# 6. Apply the update (three-way merge)
tuff update rust-implement
# installed rust-implement (open-agents) → .agents/skills/rust-implement/SKILL.md

# 7. After editing, check drift
echo "# my custom rule" >> .agents/skills/rust-implement/SKILL.md
tuff list
# rust-implement         def5678    project    open-agents    modified
```

## Use Case: Adopt Tuff in an existing project

Your repo already has agent files. Bring them under Tuff management without moving anything.

```sh frame="terminal"
# Repo already has:
#   .agents/skills/existing-skill/SKILL.md
#   .agents/tools/scan-tool/index.js
#   .claude/skills/claude-only/SKILL.md

# 1. Initialize Tuff
tuff init
# existing .agents/ directories stay untouched
# .agents/workflows/ created (new: Tuff introduces this)

# 2. Register your agents
tuff agent add open-agents
tuff agent add claude

# 3. Add the existing directories in place
tuff add --agent open-agents .agents/skills/existing-skill
tuff add --agent open-agents .agents/tools/scan-tool
tuff add --agent claude .claude/skills/claude-only
# added existing-skill (skill, open-agents) -> .agents/skills/existing-skill
# added scan-tool (tool, open-agents) -> .agents/tools/scan-tool
# added claude-only (skill, claude) -> .claude/skills/claude-only

# 4. Verify: everything tracked, zero files moved
tuff list
# tuff-cli-guide        0.1.0    project    open-agents    clean
# existing-skill         0.1.0    project    open-agents    clean
# scan-tool              0.1.0    project    open-agents    clean
# claude-only            0.1.0    project    claude         clean

# 5. CI is ready
tuff check
# ✓ tuff-cli-guide        skill    open-agents    ok
# ✓ existing-skill         skill    open-agents    ok
# ✓ scan-tool              tool     open-agents    ok
# ✓ claude-only            skill    claude         ok
```

## Directory model

| Directory | Purpose | Created by |
|---|---|---|
| `.agents/skills/` | Skill runtime files | `tuff init` scaffolds, you author |
| `.agents/tools/` | Tool runtime files | Same |
| `.agents/hooks/` | Hook runtime files | Same |
| `.agents/workflows/` | Workflow runtime files | Same |
| `tuff.lock` | Committed capability identity and lifecycle metadata | `tuff init` |
| `.claude/skills/` etc. | Claude-specific capabilities | `tuff add` or you author |

**One directory, one source of truth.** Agent files live in `.agents/` or `.claude/`.
Tuff tracks them in place, with no copies or duplication.
