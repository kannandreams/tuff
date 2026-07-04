---
title: Skills
description: Skills teach an agent how to work inside a project.
---

A skill is project-specific instruction. It tells an agent how to work in a repository,
what conventions matter, what workflows are expected, and what domain context it should keep in mind.

In the current Coral implementation, skills are the first supported primitive kind.
They are installed into Codex-style targets as:

```text
.agents/skills/<id>/SKILL.md
```

Coral's goal is not just to copy skill files. It tracks where they came from, records a baseline,
and reports local drift after installation.
