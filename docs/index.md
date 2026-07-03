# Loadout

<div class="loadout-hero">
  <span class="loadout-hero-label">Agent capabilities lifecycle manager.</span>
  <p>
    Loadout is a CLI for managing project-owned skills, tools, hooks, and
    workflows, then loading them into coding harnesses such as Codex, Claude,
    Cursor, and others.
  </p>
</div>

## Why Loadout exists

Agent coding tools increasingly depend on shared team context: skills, rules,
hooks, workflows, setup notes, and scripts that encode how a project should be
changed. Today those assets often become copied files, hidden global config, or
harness-specific folders that drift silently over time.

That works for one engineer. It breaks when a team needs provenance, review,
validation, updates, and portability.

Loadout gives those assets a lifecycle:

1. Install a capability from a local source or pack.
2. Record the installed baseline.
3. Let the project owner customize the installed artifact.
4. Detect local drift.
5. Show diffs against the recorded baseline.
6. Build toward safe update and merge behavior.

The first implementation proves this loop with Codex-style skills installed at
`.agents/skills/<id>/SKILL.md`.

## The problem

| What happens today | How Loadout solves it |
| --- | --- |
| **Copy/paste drift:** skills and rules are copied into many projects and diverge silently. | Record a baseline and report local drift. |
| **No provenance:** teams cannot tell where an agent asset came from. | Store source and version metadata in the lockfile. |
| **Hidden machine state:** global config works for one engineer but not the team. | Keep project-owned, reviewable files. |
| **Harness lock-in:** each agent expects a different file layout. | Compile managed capabilities through harness adapters. |
| **Unsafe execution:** tools and hooks can run commands without clear review. | Add validation, policies, and dry-run planning. |
| **No update path:** updates can overwrite local customization. | Build toward baseline/local/upstream merge support. |
| **Weak onboarding:** new projects rely on tribal setup knowledge. | Provide repeatable CLI commands and packs. |

## Core idea

Loadout core is content-agnostic. It is the engine for manifests, install
state, lockfiles, drift detection, diffs, validation, and harness adapters.

The content itself belongs outside the core engine:

- team capabilities can live in a company pack repository
- project-specific capabilities can live in the project that owns them
- optional starter packs can provide defaults without becoming core behavior

Loadout uses the internal term `primitive` for a managed artifact. In public
docs, the friendlier term is `capability`: a skill, tool, hook, or workflow
that a team manages with Loadout.

## Current scope

Loadout currently supports:

- local filesystem capabilities
- `kind = "skill"`
- `target = "codex"`
- install, list, drift detection, and baseline diff

Remote packs, tools, hooks, workflows, multi-harness compilation, update checks,
and merge behavior are tracked on the [roadmap](roadmap.md).

## Inspiration

Loadout is inspired by [skills.sh](https://skills.sh/) and the idea that
reusable skills should be easy to share, install, and use across coding agents.

That solves an important first step: making skills portable. Loadout focuses on
what happens after a team starts depending on those skills in real projects.
Teams need to know where a skill came from, how it changed, whether local edits
are intentional, and how to load it consistently into different coding
harnesses.

Loadout starts with reusable skills, then applies the same lifecycle model to
tools, hooks, and workflows: baseline tracking, drift detection, validation,
adapter-based output, and future merge behavior.
