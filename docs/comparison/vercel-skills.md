# Vercel Skills vs Loadout

Vercel Skills and Loadout overlap on the basic idea that agent context should
be installable and reusable across repositories. The strategic difference is
that Vercel Skills is already a broad skill ecosystem, while Loadout is aiming
at typed primitive lifecycle management.

## Summary

Loadout should not compete head-on as another `npx skills` clone. Vercel has
already shipped the broad skill installation and discovery layer.

Loadout's wedge is narrower:

- model tools, hooks, and workflows as first-class primitives, not only skills
- treat installed artifacts as repo-owned files
- track baselines so local drift is visible
- build toward real update and merge behavior for customized primitives

The current Codex skill support is a proof path for the lifecycle mechanic, not
the full product thesis.

## Comparison

| Area | Vercel Skills | Loadout |
| --- | --- | --- |
| Primary scope | Open skill ecosystem for reusable agent context | Typed lifecycle manager for repo-local agent primitives |
| Current primitive model | Skills, typically `SKILL.md` plus supporting files | Starts with skills, designed to expand to tools, hooks, and workflows |
| Distribution | Public git repos, local folders, and ecosystem discovery | Local filesystem primitives first; registry behavior deferred |
| CLI shape | `npx skills add`, `find`, `list`, `remove`, `update`, `init`, and related commands | `loadout init`, `add`, `list`, and `diff` for the first lifecycle loop |
| Agent targets | Broad multi-agent support across many coding assistants | Codex target first; multi-target support is not the near-term wedge |
| Install ownership | Project or global scope, with copy/symlink behavior | Project-local artifacts are repo-owned after install |
| Drift handling | Update behavior exists, but local-edit merge semantics are not the core positioning | Baseline hashes and diffs are central; future update/merge behavior is the product bet |
| Opinion layer | Broad ecosystem and discovery directory | Curated typed primitives and opinionated packs can sit above the engine |
| Best use case | Finding and installing reusable skills across many agents | Managing locally customized primitives over time without losing upstream context |

## Strategic take

Vercel Skills substantially covers the broad Layer 1 and Layer 2 idea for
skills: installation, discovery, updates, and cross-agent output. Rebuilding
that as a generic marketplace would be a weak starting point.

Loadout should go deeper where the skill ecosystem is thinner:

- typed primitives beyond markdown skills
- lifecycle state that makes local customization explicit
- diffs and future merges against known baselines
- curated conventions where the opinion matters as much as the packaging

That direction makes Loadout complementary to the broader ecosystem rather than
a smaller clone of it.

## Sources

- [Vercel Skills changelog](https://vercel.com/changelog/introducing-skills-the-open-agent-skills-ecosystem)
- [Vercel guide to creating, installing, and sharing agent skills](https://vercel.com/kb/guide/agent-skills-creating-installing-and-sharing-reusable-agent-context)
- [Docusaurus installation documentation](https://docusaurus.io/docs/installation)
