---
title: Overview
description: What Coral primitives are and how they fit together.
---

Coral uses the internal term `primitive` for any managed agent-facing building block.
A primitive defines some part of an agent's operating surface: what it knows, what it can do,
what rules it must obey, and what multi-step flows it should follow.

In public docs, the friendlier term is often `capability`. The useful distinction is:

- `primitive` describes the schema and lifecycle unit Coral manages
- `capability` describes the practical thing a team installs and maintains

Coral is designed to manage these primitive kinds:

- skills
- tools
- hooks
- policies
- workflows

The current implementation proves the lifecycle loop with Codex-style skills first. The other
primitive kinds are product direction and roadmap work, but the docs describe them now because
they are part of the intended model.
