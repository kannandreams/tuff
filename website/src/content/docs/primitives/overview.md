---
title: Overview
description: What Coral primitives are and how they fit together.
---

Coral uses the term `primitive` for any managed agent-facing building block.
A capability defines some part of an agent's operating surface: what it knows, what it can do,
what rules it must obey, and what multi-step flows it should follow.

Coral manages these capability types:

| Kind | Status | Description |
|---|---|---|
| [skill](/primitives/skills) | **Implemented** | Prose instruction injected into agent context |
| [tool](/primitives/tools) | **Implemented** | Executable capability with typed parameter contract |
| [hook](/primitives/hooks) | **Implemented** | Event-driven automation at lifecycle moments |
| policy | Roadmap | Constraints and guardrails on agent behavior |
| workflow | Roadmap | Multi-step processes composed from other primitives |
