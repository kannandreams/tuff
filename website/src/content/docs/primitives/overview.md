---
title: Overview
description: What Tuff primitives are and how they fit together.
---

Tuff uses the term `capability` for any managed agent-facing building block. A capability defines some part of an agent's operating surface: what it knows, what it can do, what rules it must obey, and what multi-step flows it should follow.

Tuff manages these capability types:

<div class="capability-types-table">

| Capability | Description | Feature status |
|---|---|---|
| [skill](/primitives/skills) | Prose instruction injected into agent context | **Implemented** |
| [tool](/primitives/tools) | Executable capability with typed parameter contract | **Implemented** |
| [hook](/primitives/hooks) | Event-driven automation at lifecycle moments | **Implemented** |
| [workflow](/primitives/workflows) | Composable patterns bundling skills, tools, and hooks | Roadmap |
| policy | Constraints and guardrails on agent behavior | Roadmap |

</div>
