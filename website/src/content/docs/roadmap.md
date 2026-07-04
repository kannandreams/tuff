---
title: Roadmap
description: Planned lifecycle, capability, and distribution work for Coral.
---

This roadmap tracks the public direction for Coral. The sequence is designed
to keep the core lifecycle stable before expanding into executable primitives,
remote distribution, and policy.

## Product direction

Coral should solve lifecycle management for agent capabilities:

- install capabilities into a project
- record where they came from
- detect local drift
- validate structure and safety
- compile to coding harnesses
- eventually update and merge without destroying local customization

The core CLI should remain content-agnostic. Opinionated skills, tools, hooks,
and workflows belong in packs or in the projects that own them.

## Core roadmap

| Order | Feature | Status |
| ---: | --- | --- |
| 0 | [Complete core primitive engine MVP](https://github.com/kannandreams/loadout/issues/5) | Planned |
| 1 | [Harness adapter abstraction](https://github.com/kannandreams/loadout/issues/2) | Planned |
| 2 | [Override and scope resolution](https://github.com/kannandreams/loadout/issues/3) | Planned |
| 3 | [Baseline diff and three-way merge lifecycle](https://github.com/kannandreams/loadout/issues/6) | Planned |
| 4 | [Tool primitive type](https://github.com/kannandreams/loadout/issues/4) | Planned |
| 5 | [Hook primitive type](https://github.com/kannandreams/loadout/issues/7) | Planned |
| 6 | [Workflow primitive type](https://github.com/kannandreams/loadout/issues/8) | Planned |
| 7 | [Pack and registry distribution model](https://github.com/kannandreams/loadout/issues/9) | Planned |
| 8 | [Lightweight update checks](https://github.com/kannandreams/loadout/issues/10) | Planned |
| 9 | [Validation and install-time safety framework](https://github.com/kannandreams/loadout/issues/11) | Planned |

## Adoption and operations backlog

These features make Coral easier to adopt in real teams after the main
lifecycle foundation is in place.

| Feature | Why it matters |
| --- | --- |
| [Import and adopt existing agent assets](https://github.com/kannandreams/loadout/issues/12) | Teams already have copied skills, scripts, rules, and external assets. |
| [Project diagnostics with `coral doctor`](https://github.com/kannandreams/loadout/issues/13) | Engineers need one command to explain broken state. |
| [CI-friendly validation mode](https://github.com/kannandreams/loadout/issues/14) | Agent capability changes should be enforceable in pull requests. |
| [Policy file and trust controls](https://github.com/kannandreams/loadout/issues/15) | Teams need guardrails for remote sources, hooks, tools, and scopes. |
| [Capability ownership and review metadata](https://github.com/kannandreams/loadout/issues/16) | Sensitive capabilities need clear maintainers and reviewers. |
| [Deprecation and migration metadata](https://github.com/kannandreams/loadout/issues/17) | Teams need a clean path away from old or unsafe capabilities. |
| [Provenance, checksums, and future signing support](https://github.com/kannandreams/loadout/issues/18) | Remote and executable capabilities need auditable source integrity. |

## Deferred intentionally

These are useful later, but should not distract from the lifecycle engine:

- hosted marketplace UI
- automatic hook/tool execution
- organization policy server
- package signing infrastructure
- broad multi-harness support before the adapter model is stable
- a bundled opinionated standard pack inside the core repo
