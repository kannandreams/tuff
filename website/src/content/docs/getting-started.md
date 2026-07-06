---
title: Getting Started
description: Initialize Coral state and install your first capability.
---

This guide walks through setting up Coral and managing capabilities in a project.

## 1. Initialize Coral

```sh
coral init
```

Creates `.coral/` state, scaffolds `.agents/` directories, and auto-installs
`coral-cli-guide` — a reference skill your coding agent can read to learn coral commands.

## 2. Register harness targets

```sh
coral target add open-agents
coral target add claude
```

## 3. Create and track your first capability

```sh
# Create a skill directly in the agent directory
mkdir -p .agents/skills/my-skill
cat > .agents/skills/my-skill/coral.toml << 'EOF'
id = "my-skill"
version = "1.0.0"
type = "skill"
description = "My first Coral-managed capability."
files = ["SKILL.md"]
EOF
echo "# My Skill\n\nProject conventions go here." > .agents/skills/my-skill/SKILL.md

# Track it with Coral (records baseline, no files copied — tracked in-place)
coral import .agents/skills/my-skill -t open-agents
```

## 4. Inspect

```sh
coral list
# coral-cli-guide    0.1.0    project    open-agents    clean
# my-skill           0.1.0    project    open-agents    clean
```

## 5. Edit and detect drift

Edit `.agents/skills/my-skill/SKILL.md`, then:

```sh
coral list
# my-skill    0.1.0    project    open-agents    modified

coral diff my-skill
# shows what changed
```

## 6. Install from git

```sh
coral add https://github.com/pproenca/dot-skills --skill rust-implement -t open-agents
coral outdated
coral update rust-implement
```

## 7. CI validation

```sh
coral check
# ✓ coral-cli-guide    skill    open-agents    ok
# ✓ my-skill           skill    open-agents    ok
```

See the [development lifecycle guide](/concepts/development-lifecycle) for full scenario walkthroughs.
