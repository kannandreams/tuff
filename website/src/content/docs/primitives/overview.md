---
title: What Is a Capability
description: The building blocks Tuff manages, and how they fit together.
---

Tuff uses the word **capability** for any building block you give a coding agent. Together, capabilities shape how an agent works in a project: what it knows, what it can do, what rules it must obey, and which multi-step flows it should follow.

Tuff manages these capability types:

<div class="capability-types-table">

| Capability | What it is | Feature status |
|---|---|---|
| [skill](/primitives/skills) | Written instructions the agent reads | **Implemented** |
| [tool](/primitives/tools) | Something the agent can run, with typed parameters | **Implemented** |
| [mcp-server](/primitives/mcp-servers) | An external MCP server, wired into every agent from one declaration | **Implemented** |
| [hook](/primitives/hooks) | A command that runs automatically at a moment such as session start | **Implemented** |
| [policy](/primitives/policies) | Rules that narrow what an agent may do: commands, file reads and edits, MCP tools | Preview |

</div>

Every type is handled the same way. You install a capability once, and Tuff:

- writes it into each agent's own folder, in that agent's format,
- records what it wrote in `tuff.lock`,
- tells you when the installed files change or a newer version is available.

A capability usually describes itself in a [`tuff.toml`](/primitives/format) file. Several capabilities can be bundled and shipped together as a [pack](/concepts/packs/).
