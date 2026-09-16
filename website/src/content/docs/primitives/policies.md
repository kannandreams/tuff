---
title: Policies
description: Policies declare what an agent must never do, or must ask before doing, once for every harness.
---

A policy capability is a list of rules that narrow what a coding agent may do in a project: commands it must not run, files it must not read or edit, MCP tools it must not call, and actions it must ask a person about first. It is written once, and each agent the project uses enforces it in its own way, or Tuff says plainly that it cannot.

:::caution[Preview: Claude Code, OpenCode, and Codex]
Claude Code and OpenCode enforce every kind of policy rule. Codex enforces `command` rules only. Tuff turns each rule into the agent's own rules.

If you install a policy for an agent that does not enforce one of its rules, such as Cursor, or Codex for a `read` rule, `tuff add` stops with an error and installs nothing. The error lists the rules that agent cannot enforce.

This is on purpose. If Tuff installed the policy anyway, the agent would ignore the rules, but you would think they were in place.

`--accept-unenforced` does not change this for an agent that enforces none of the policy's rules. See [Rules an agent does not enforce](#rules-an-agent-does-not-enforce).
:::

The recording below runs the [policy guardrails example](https://github.com/kannandreams/tuff-pack-examples/tree/main/projects/policy-guardrails). Claude Code is asked for a value in `.env` and reads the file. `tuff add` then installs a policy that denies reading `.env`, and the same question is denied.

<video controls muted playsinline preload="none" poster="/video/policy-guardrails-demo.png" width="1600" height="800" style="width:100%;height:auto;border-radius:0.5rem;">
  <source src="/video/policy-guardrails-demo.mp4" type="video/mp4" />
  <a href="/video/policy-guardrails-demo.mp4">Download the recording</a> if your browser cannot play it inline.
</video>

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

### What happens to your settings file

- **Your existing settings stay.** Tuff adds its rules next to the permission rules and settings already in `.claude/settings.json`. A rule that is already there is not added again.
- **A broken file is not touched.** If Tuff cannot read `.claude/settings.json`, it stops before writing anything.
- **Tuff remembers which rules it added.** Each rule is recorded in `tuff.lock`, so later commands only touch those rules:

| Command | What it does with the policy's rules |
|---|---|
| `tuff check` | Reports a rule that was deleted from the file by hand |
| `tuff update` | Removes rules the updated policy no longer has |
| `tuff delete` | Removes the rules this policy added, and nothing else |

### How much each rule protects

| Rule | Coverage | Why |
|---|---|---|
| `mcp` | `full` | The rule names the MCP tool, so every call to it is caught |
| `command` | `partial` | Only the command as Claude writes it is caught |
| `read`, `edit` | `partial` | Only Claude's own file tools and common shell commands are caught |

A **command rule** catches the command even inside a longer one, such as
`cd x && git push --force`. It does not catch the same action written a
different way:

- with the program's full path, such as `/usr/bin/git push --force`
- inside another shell, such as `sh -c "git push --force"`
- with options before the subcommand, such as `git -C . push --force`

A **file rule** covers Claude's file tools and shell commands Claude Code
recognises, such as `cat` and `sed`. It does not stop a script or program
that opens the file itself.

This limit comes from Claude Code, not Tuff: Claude Code matches the text of
a command, and its own documentation says command and file rules are not a
security boundary.

:::caution[Use the sandbox when a rule must never be bypassed]
Policies stop an agent from doing something harmful by accident, such as a
force push or reading `.env`. They are not a guarantee.

If a command or file must never be reached, also turn on Claude Code's
sandbox, which blocks it at the operating-system level however it is written.
Tuff cannot turn the sandbox on for you, because no file in the repository
controls it.
:::

## Codex

Tuff compiles `command` rules into Codex's command rules, in a file Tuff owns at `.codex/rules/tuff.rules`:

| Policy rule | Codex rule |
|---|---|
| `effect = "deny"`, `command = ["git", "push", "--force"]` | `prefix_rule(pattern = ["git", "push", "--force"], decision = "forbidden")` |
| `effect = "ask"`, `command = ["terraform", "apply"]` | `prefix_rule(pattern = ["terraform", "apply"], decision = "prompt")` |

A rule's `reason` becomes the rule's `justification`, which Codex shows when it refuses the command. These mappings were checked against Codex CLI 0.154.0.

- **Trust.** Codex loads project rules only when the project is trusted. Until then the file is written but not applied.
- **Experimental.** Codex's documentation labels rules experimental.
- **Matching.** Codex matches a command's leading words, and splits a simple chain such as `git add . && git push --force` to check each command. A script with redirection, `$(...)`, a variable assignment, a wildcard, or control flow is checked as one command, so `git push --force > push.log` is not matched. A program run by absolute path, such as `/usr/bin/git`, may not be matched.
- **Ask without approvals.** Where Codex never asks for approval, as in `codex exec` by default, a `prompt` rule refuses the command.
- **Other rules.** Codex rules match commands, not file paths or MCP tools, so `read`, `edit`, and `mcp` rules are not enforced in Codex. A policy with such rules installs for Codex only with [`--accept-unenforced`](#rules-an-agent-does-not-enforce).

`tuff check` reports a compiled rule removed from the file by hand, and `tuff delete` removes the policy's rules and the file once no rules remain. To see how Codex reads a rule:

```sh frame="terminal"
codex execpolicy check --rules .codex/rules/tuff.rules -- git push --force
```

## OpenCode

Tuff compiles every kind of rule into OpenCode's `permission` settings, in `.opencode/opencode.json`. Add the `opencode` agent first with `tuff agent add opencode`.

| Policy rule | OpenCode rule in `permission` |
|---|---|
| `command = ["git", "push", "--force"]` | `"bash": {"git push --force *": "deny"}` |
| `read = [".env"]` | `"read": {".env": "deny", "*/.env": "deny"}` |
| `read = ["secrets/**"]` | `"read": {"secrets/**": "deny"}` |
| `edit = ["infra/prod/"]` | `"edit": {"infra/prod/*": "deny"}` |
| `mcp = "github:delete_*"` | `"github_delete_*": "deny"` |

An `ask` rule is written with `"ask"` in place of `"deny"`.

- **Precedence.** OpenCode applies the last permission rule that matches. It loads `.opencode/opencode.json` after the project's `opencode.json`, and Tuff adds its rules after the rules already in `.opencode/opencode.json`, `ask` before `deny`. The policy's rules therefore take precedence over the project's own. Inline `OPENCODE_CONFIG_CONTENT`, managed configuration, and an agent's own `permission` settings are applied later and can still override them.
- **Your files.** Tuff keeps the keys, rules, and order already in `.opencode/opencode.json`, and stops with an error if that file has a rule for the same pattern with a different action. It does not edit `opencode.json` or `.opencode/opencode.jsonc`.
- **Commands.** OpenCode checks each command it parses from the shell input, so `cd x && git push --force` and `git push --force > push.log` are matched. `sh -c "git push --force"`, `/usr/bin/git push --force`, and `git -C . push --force` are not.
- **Files.** OpenCode matches the path relative to the project, so a pattern without a `/`, such as `.env`, becomes two OpenCode patterns. A `read` rule covers OpenCode's read tool, and an `edit` rule its edit, write, and patch tools. `grep`, `glob`, `list`, and shell commands are separate permissions and are not covered.
- **MCP tools.** A denied tool is hidden from the agent. OpenCode names a tool `<server>_<tool>`, with characters other than letters, digits, `_`, and `-` replaced by `_`.
- **Ask.** `opencode run` rejects the request an `ask` rule raises, and `opencode --auto` approves it.

`opencode` takes policies and [MCP servers](/primitives/mcp-servers/#what-gets-written). Skills reach OpenCode through `open-agents`, and `tuff init` does not register `opencode`.

## What each agent can enforce

```sh frame="terminal"
tuff policy matrix
tuff policy matrix --json
```

Example output, showing Claude Code and Cursor. The full output also lists
Open Agents, whose rows read `unsupported` like Cursor's, Codex, whose
`command` rows read `partial`, and OpenCode, whose rows read like Claude Code's:

```text
┌─────────────┬────────┬─────────┬─────────────┬────────────────────────────────────────┐
│ ADAPTER     │ EFFECT │ SUBJECT │ COVERAGE    │ MECHANISM                              │
├─────────────┼────────┼─────────┼─────────────┼────────────────────────────────────────┤
│ claude      │ deny   │ command │ partial     │ permissions.deny Bash(<command> *)     │
│ claude      │ deny   │ read    │ partial     │ permissions.deny Read(<path>)          │
│ claude      │ deny   │ edit    │ partial     │ permissions.deny Edit(<path>)          │
│ claude      │ deny   │ mcp     │ full        │ permissions.deny mcp__<server>__<tool> │
│ claude      │ ask    │ command │ partial     │ permissions.ask Bash(<command> *)      │
│ claude      │ ask    │ read    │ partial     │ permissions.ask Read(<path>)           │
│ claude      │ ask    │ edit    │ partial     │ permissions.ask Edit(<path>)           │
│ claude      │ ask    │ mcp     │ full        │ permissions.ask mcp__<server>__<tool>  │
│ cursor      │ deny   │ command │ unsupported │                                        │
│ cursor      │ deny   │ read    │ unsupported │                                        │
│ cursor      │ deny   │ edit    │ unsupported │                                        │
│ cursor      │ deny   │ mcp     │ unsupported │                                        │
│ cursor      │ ask    │ command │ unsupported │                                        │
│ cursor      │ ask    │ read    │ unsupported │                                        │
│ cursor      │ ask    │ edit    │ unsupported │                                        │
│ cursor      │ ask    │ mcp     │ unsupported │                                        │
└─────────────┴────────┴─────────┴─────────────┴────────────────────────────────────────┘

Notes:
- claude: matches the command as Claude writes it, including inside compound commands; the same program run another way, such as by absolute path, through sh -c, or as git -C . push, is not matched
- claude: covers Claude's file tools and the shell commands Claude Code recognises, such as cat and sed, not a script or program that opens the file itself
- cursor: Tuff does not compile policy rules for this agent yet
```

How to read it:

| Column | What it means |
|---|---|
| `ADAPTER` | The agent |
| `EFFECT` | `deny` or `ask` |
| `SUBJECT` | The kind of rule: `command`, `read`, `edit`, or `mcp` |
| `COVERAGE` | `full` (always enforced), `partial` (enforced with the limits in the notes), or `unsupported` (not enforced, so `tuff add` refuses the policy) |
| `MECHANISM` | What Tuff writes for that agent, such as a Claude Code permission rule |

The matrix has one row per agent, effect, and subject, with the same `full`, `partial`, and `unsupported` coverage the [Hooks Specification](/spec/hooks/) uses for hooks, the mechanism a rule compiles to, and the caveat when coverage is partial. `tuff add` prints each partial caveat for the rules it installs, and refuses a policy for any selected agent that would not enforce one of its rules. Cursor and Open Agents enforce nothing yet, Codex enforces `command` rules only, and OpenCode enforces every kind of rule.

## Rules an agent does not enforce

By default, `tuff add` refuses a policy when a selected agent does not enforce one of its rules, and installs nothing. `--accept-unenforced` installs the rules each agent enforces instead, and records the others in `tuff.lock`:

```sh frame="terminal"
tuff add ./policies/infra-guardrails --agent <agent> --accept-unenforced
```

- `tuff add` prints each rule it did not install, with the agent and the reason.
- An agent that enforces none of the policy's rules still refuses the policy, with or without the flag.
- `tuff check` prints every recorded rule on each run, and exits 0 for them.
- `tuff check --strict` exits 1 while any rule is recorded, and `tuff check --json` lists them under `gaps`.
- `tuff update` recomputes the record without the flag. A rule the agent enforces in a newer Tuff, or a rule removed from the policy, leaves the record.

`--accept-unenforced` applies to `tuff add <path>`. Typed commands such as `tuff add skill` refuse it.

## Where policies do not go

A policy is not written into `AGENTS.md`, `CLAUDE.md`, or any other instruction file. Those are read by the model as advice and enforce nothing, which is exactly the gap a policy exists to close.
