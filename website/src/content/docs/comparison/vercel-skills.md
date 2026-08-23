---
title: Skills.sh and Vercel Skills vs Tuff
description: Where Tuff differs from the broader skills ecosystem.
---

Skills.sh and Vercel Skills helped validate the basic idea that agent context
should be installable and reusable across repositories. Tuff is inspired by
that direction, but it is not trying to be a smaller clone of the same
marketplace.

The strategic difference is that Vercel Skills is a broad skill ecosystem,
while Tuff is aiming at lifecycle management for project-owned agent
capabilities.

## Summary

Tuff should not compete head-on as another `npx skills` clone. Vercel has
already shipped the broad skill installation and discovery layer.

Tuff's wedge is broader and more operational:

- model tools, hooks, and workflows as first-class capabilities, not only skills
- treat installed artifacts as repo-owned files
- track baselines so local drift is visible
- build toward real update and merge behavior for customized primitives

The current Codex skill support is a proof path for the lifecycle mechanic, not
the full product thesis.

## Comparison

| Area | Vercel Skills | Tuff |
| --- | --- | --- |
| Primary scope | Open skill ecosystem for reusable agent context | Lifecycle manager for project-owned agent capabilities |
| Current model | Skills, typically `SKILL.md` plus supporting files | Starts with skills, designed to expand to tools, hooks, and workflows |
| Distribution | Public git repos, local folders, and ecosystem discovery | Local and Git capability sources plus deterministic multi-capability pack artifacts; registry transport deferred |
| CLI shape | `npx skills add`, `find`, `list`, `remove`, `update`, `init`, and related commands | `tuff init`, `add`, `list`, and `diff` for the first lifecycle loop |
| Agents | Broad multi-agent support across many coding assistants | Codex first; multiple-agent support is not the near-term wedge |
| Install ownership | Project or global scope, with copy/symlink behavior | Project-local artifacts are repo-owned after install |
| Drift handling | Update behavior exists, but local-edit merge semantics are not the core positioning | Baseline hashes and diffs are central; future update/merge behavior is the product bet |
| Opinion layer | Broad ecosystem and discovery directory | Versioned packs sit above the content-agnostic per-capability lifecycle |
| Best use case | Finding and installing reusable skills across many agents | Managing locally customized capabilities over time without losing upstream context |

## Strategic take

Vercel Skills substantially covers the broad Layer 1 and Layer 2 idea for
skills: installation, discovery, updates, and cross-agent output. Rebuilding
that as a generic marketplace would be a weak starting point.

Tuff should go deeper where the skill ecosystem is thinner:

- typed capabilities beyond markdown skills
- lifecycle state that makes local customization explicit
- diffs and future merges against known baselines
- optional packs where curated conventions can evolve outside core

That direction makes Tuff complementary to the broader ecosystem rather than
a smaller clone of it.

## Sources

- [Vercel Skills changelog](https://vercel.com/changelog/introducing-skills-the-open-agent-skills-ecosystem)
- [Vercel guide to creating, installing, and sharing agent skills](https://vercel.com/kb/guide/agent-skills-creating-installing-and-sharing-reusable-agent-context)
- [Skills.sh](https://skills.sh/)
- [Docusaurus installation documentation](https://docusaurus.io/docs/installation)
