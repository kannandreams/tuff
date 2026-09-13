---
title: Policies
description: Policies declare what an agent must never do, or must ask before doing, once for every harness.
---

A policy capability is a list of rules that narrow what a coding agent may do in a project: commands it must not run, files it must not read or edit, MCP tools it must not call, and actions it must ask a person about first. It is written once, and each agent the project uses enforces it in its own way, or Tuff says plainly that it cannot.

:::caution[Preview]
Claude Code enforces policies today, through its own permission rules. For every other agent, `tuff add` refuses a policy and names each rule that agent would not enforce, rather than installing it and letting you believe it works.
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
| `command` | A shell command, as a prefix of its arguments, each argument its own string. `["git", "push", "--force"]` matches `git push --force` and `git push --force origin main`. Arguments are literal words; `*` is not allowed. |
| `read` | Path patterns the agent must not read, such as `".env"` or `"secrets/**"`. |
| `edit` | Path patterns the agent must not edit or write. |
| `mcp` | An MCP tool as `"server:tool"`, where either side may use `*`, such as `"github:delete_*"`. |
| `reason` | Optional. Why the rule exists. |

Path patterns follow `.gitignore` rules as if the file sat at the project root: a pattern with a `/` at its start or middle, such as `secrets/**`, applies where it is written; one without, such as `.env` or `*.pem`, applies at any depth; and a trailing `/`, such as `certs/`, covers everything in that directory.

A policy installs no `files`. A rule with no subject, with two subjects, with an argument containing a space or `*`, or with a path that starts with `/` or `~` or climbs out with `..` is refused when the policy is loaded, and so is a misspelt field.

## A policy cannot allow anything

There is no `"allow"` effect, and a rule that uses one is refused. A policy is a capability like any other, so it can come from another team's repository or a published pack. A policy that could grant permissions could quietly widen what an agent may do in every project that installs it. A policy that can only take permissions away can at worst be too strict, and too strict is something you notice. Permissions an agent should have belong in the harness's own settings.

## Claude Code

Tuff compiles each rule into Claude Code's own permission rules in `.claude/settings.json`, which Claude Code applies in the project without the workspace trust step, since deny and ask rules only restrict:

| Policy rule | Claude Code rule | List |
|---|---|---|
| `command = ["git", "push", "--force"]` | `Bash(git push --force *)` | `deny` or `ask` |
| `read = [".env", "secrets/**"]` | `Read(/**/.env)`, `Read(/secrets/**)` | `deny` or `ask` |
| `edit = ["infra/prod/"]` | `Edit(/infra/prod/**)` | `deny` or `ask` |
| `mcp = "github:delete_*"` | `mcp__github__delete_*` | `deny` or `ask` |

The rules are merged into the file alongside everything already there. Your own permission rules and other settings are kept, a rule already present is not added twice, and a corrupt settings file is refused before anything is written. The lockfile records each compiled rule, so `tuff check` reports a rule removed by hand as drift, `tuff update` takes out the rules a changed policy no longer has, and `tuff delete` removes exactly the rules the policy added and nothing else.

Claude Code's command and file rules are real, but its own documentation says they are not a security boundary, and Tuff reports them as `partial` for that reason:

- A command rule matches the command as Claude writes it, including inside compound commands such as `cd x && git push --force`. The same program run another way, such as by absolute path, through `sh -c`, or as `git -C . push --force`, is not matched.
- A file rule covers Claude's file tools and the shell commands Claude Code recognises, such as `cat` and `sed`, not a script or program that opens the file itself.

An MCP rule names the tool itself, so Tuff reports it as `full`. For enforcement that does not depend on how a command is spelled, use Claude Code's sandbox, which a repository file does not control.

## What each agent can enforce

```sh frame="terminal"
tuff policy matrix
tuff policy matrix --json
```

The matrix has one row per agent, effect, and subject, with the same `full`, `partial`, and `unsupported` coverage the [Hooks Specification](/spec/hooks/) uses for hooks, the mechanism a rule compiles to, and the caveat when coverage is partial. `tuff add` prints each partial caveat for the rules it installs, and refuses a policy for any selected agent that would not enforce one of its rules. Cursor, Codex, and Open Agents enforce nothing yet.

## Where policies do not go

A policy is not written into `AGENTS.md`, `CLAUDE.md`, or any other instruction file. Those are read by the model as advice and enforce nothing, which is exactly the gap a policy exists to close.
