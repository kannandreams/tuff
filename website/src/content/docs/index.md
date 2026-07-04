---
title: Introduction
description: Capability lifecycle management for coding agents.
---

<div class="coral-shell coral-banner-shell">
  <div class="coral-shell-chrome">
    <span class="coral-dot coral-dot-red"></span>
    <span class="coral-dot coral-dot-yellow"></span>
    <span class="coral-dot coral-dot-green"></span>
  </div>
  <div class="coral-banner-frame">
    <img src="/img/coral-readme-banner.png" alt="Coral banner" class="coral-banner-image" />
  </div>
</div>

## Why Coral exists

Agent coding tools increasingly depend on primitives: skills that teach an agent how to work in a project, tools it can call, hooks that run at key moments, policies that constrain what it's allowed to do, and workflows that tie these together. Together, these primitives govern what an agent can access, invoke, and is forbidden from doing.

Today these primitives are either siloed on one engineer's machine, invisible to the rest of the team, or copied by hand across projects and left to drift silently once one copy changes and the others don't.

That works for one engineer. It breaks when a team needs provenance, review, validation, updates, and portability.

Coral treats each primitive as something with a lifecycle: installed, tracked against a baseline, customized, checked for drift, and eventually reconciled with upstream updates. 
The table below shows what that replaces.

## Problem : Solution

| Without Coral | With Coral |
| --- | --- |
| **Copy/paste drift:** <br> every project ends up with its own copy of a skill, and there's no way to keep them in sync.| Coral records the exact version installed as a baseline, so any later edit shows up as detectable drift instead of silently forking. |
| **No provenance:** <br> a teammate finds a skill file in the repo with no way to tell which pack it came from, what version it is, or whether it's safe to touch. | Every install writes source and version metadata to the lockfile, so provenance is a lookup, not a guess. |
| **Hidden machine state:** <br> one engineer's global config quietly makes the agent behave differently on their machine than everyone else's, and nobody notices until something breaks in review. | Coral keeps primitives project-owned and committed, so what the agent can do is visible in the diff, not buried on one person's laptop. |
| **Harness lock-in:** <br> a skill written for one coding agent's file layout doesn't work in another, so switching tools means rewriting everything from scratch. | Coral compiles the same managed primitive into whatever file layout each harness expects, through harness adapters. |
| **Unsafe execution:** <br> a tool or hook runs with whatever permissions the harness grants it, and nothing stops it from taking a destructive or unintended action before it happens. | Policies define what a tool or hook is never allowed to do, enforced before the action runs, not discovered after. |
| **No update path:** <br> an engineer customizes an installed skill, then a later upstream fix either overwrites their edit outright, or gets skipped entirely because the file no longer matches the original and nothing knows how to reconcile the two. | Baseline, local, and upstream states merge safely, so customization and updates coexist. |
| **Weak onboarding:** <br> a new project starts from tribal knowledge: someone remembers to copy the right files from the last project, or they don't. | Repeatable CLI commands and packs turn setup into coral add <pack>, not folklore. |

## Core idea

Coral core is content-agnostic. It is the engine for manifests, install
state, lockfiles, drift detection, diffs, validation, and harness adapters.

The content itself belongs outside the core engine:

- team capabilities can live in a company pack repository
- project-specific capabilities can live in the project that owns them
- optional starter packs can provide defaults without becoming core behavior

Coral calls a managed artifact (a skill, tool, hook, policy, or workflow) a primitive. You'll sometimes see capability used instead, in the README or when describing Coral in plain language, but the docs, CLI, and manifest schema all use primitive consistently.

## Current scope

Coral currently supports:

- local filesystem capabilities
- `primitive = "skill"`
- `target = "codex"`
- install, list, drift detection, and baseline diff

Remote packs, tools, hooks, workflows, multi-harness compilation, update checks,
and merge behavior are tracked on the [roadmap](roadmap.md).

## Inspiration

Coral is inspired by [skills.sh](https://skills.sh/) and the idea that
reusable skills should be easy to share, install, and use across coding agents.

That solves an important first step: making skills portable. Coral focuses on
what happens after a team starts depending on those skills in real projects.
Teams need to know where a skill came from, how it changed, whether local edits
are intentional, and how to load it consistently into different coding
harnesses.

Coral starts with reusable skills, then applies the same lifecycle model to
tools, hooks, and workflows: baseline tracking, drift detection, validation,
adapter-based output, and future merge behavior.
