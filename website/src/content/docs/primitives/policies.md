---
title: Policies
description: Policies declare what an agent must never do, or must ask before doing, once for every harness.
---

A policy capability is a list of rules that narrow what a coding agent may do in a project: commands it must not run, files it must not read or edit, MCP tools it must not call, and actions it must ask a person about first. It is written once, and each harness the project uses is meant to enforce it in its own way.

:::caution[Preview]
The policy format and its validation are in place, and `tuff policy matrix` shows what each agent can enforce. No agent enforces policy rules through Tuff yet. Until one does, `tuff add` refuses a policy and names every rule that would not be enforced, rather than installing it and letting you believe it works.
:::

## Format

```toml title="tuff.toml"
id = "infra-guardrails"
type = "policy"
version = "1.0.0"
description = "No force pushes, no secrets, and a human approves terraform apply."

[[policy.rules]]
effect = "deny"
command = ["git", "push", "--force"]
reason = "Force pushes rewrite shared history."

[[policy.rules]]
effect = "deny"
read = [".env", "secrets/**"]

[[policy.rules]]
effect = "ask"
command = ["terraform", "apply"]

[[policy.rules]]
effect = "deny"
mcp = "github:delete_*"
```

Each `[[policy.rules]]` entry has an `effect` and exactly one subject.

| Field | Meaning |
|---|---|
| `effect` | `"deny"` refuses the action; `"ask"` requires a person to approve it first. |
| `command` | A shell command, as a prefix of its arguments, each argument its own string. `["git", "push", "--force"]` matches `git push --force origin main`. |
| `read` | Path patterns, relative to the project root, that the agent must not read, such as `".env"` or `"secrets/**"`. |
| `edit` | Path patterns, relative to the project root, that the agent must not edit or write. |
| `mcp` | An MCP tool as `"server:tool"`, where either side may use `*`, such as `"github:delete_*"`. |
| `reason` | Optional. Why the rule exists. |

A policy installs no `files`. A rule with no subject, with two subjects, with an argument containing a space, or with a path that starts with `/` or `~` or climbs out with `..` is refused when the policy is loaded, and so is a misspelt field.

## A policy cannot allow anything

There is no `"allow"` effect, and a rule that uses one is refused. A policy is a capability like any other, so it can come from another team's repository or a published pack. A policy that could grant permissions could quietly widen what an agent may do in every project that installs it. A policy that can only take permissions away can at worst be too strict, and too strict is something you notice. Permissions an agent should have belong in the harness's own settings.

## What each agent can enforce

```sh frame="terminal"
tuff policy matrix
tuff policy matrix --json
```

The matrix has one row per agent, effect, and subject, with the same `full`, `partial`, and `unsupported` coverage the [Hooks Specification](/spec/hooks/) uses for hooks, and the mechanism a rule compiles to. Today every row is `unsupported`.

Expect most rows to read `partial` even once enforcement arrives. The agents' own documentation describes their permission rules and hooks as guardrails rather than security boundaries. Claude Code's, for example, notes that a rule denying `git push` does not stop the same command run as `sh -c 'git push'`, and Cursor's and Codex's hooks let an action through when a hook fails unless configured otherwise. Tuff will report each of those limits for each rule when you install a policy, instead of leaving them in documentation nobody reads at the moment it matters.

## Where policies do not go

A policy is not written into `AGENTS.md`, `CLAUDE.md`, or any other instruction file. Those are read by the model as advice and enforce nothing, which is exactly the gap a policy exists to close.
