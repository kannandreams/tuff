---
name: cli-first-tool-design
description: Design and standardize engineering CLI command surfaces, workflows, flags, subcommands, and deterministic execution before adding richer interfaces.
allowed-tools: [Read, Write]
---

# CLI-First Tool Design Skill

## When to invoke this skill

Use this skill to design or review an engineering tool's command-line
interface before adding richer interfaces. The CLI should prove the core
workflow, establish stable command semantics, and make state-changing
behavior predictable.

Do NOT use this for API design, UI design, or non-CLI interfaces — this skill
is specifically about validating a tool's behavior through deterministic,
batch-oriented command-line execution.

## Inputs

- tool goal
- primary user tasks
- core data flow
- output artifacts
- constraints

## Outputs

- CLI-first implementation plan
- initial command surface
- execution boundaries
- staged interface roadmap
- explicit non-goals for v1

## Command-surface architecture

- Use verbs for actions and positional arguments for primary identities:
  `tool create skill <id>` is clearer than several mutually exclusive flags.
- Use subcommands for mutually exclusive resource types or behavior variants.
- Use flags for orthogonal modifiers, configuration, filters, scope, and
  destinations.
- Keep short flags consistent across commands. Reuse a short flag only when
  its meaning is stable, such as `-t, --target`.
- Decide cardinality explicitly: required, optional, defaulted, or repeatable.
  Repeat selectors when the same operation can apply to multiple targets.
- Prefer one clear responsibility per command. A command that creates,
  installs, tracks, or adopts files must say which lifecycle transition it
  performs.
- Avoid workflows that require an unexplained follow-up command. If a command
  creates a managed artifact, initialize required state and record ownership;
  reserve `import` or `adopt` for artifacts created outside the tool.

## Rules

- Prove the workflow in the CLI before building UI layers.
- Keep commands human-readable and predictable.
- Prefer deterministic output and explicit file locations.
- Avoid async, live, or interactive complexity until the batch workflow works.
- Keep modules small and composable.
- Validate all inputs and conflicts before mutating files or external state.
- Make output deterministic: report canonical IDs, explicit paths, and the
  next useful command.
- Define artifact ownership, source paths, generated outputs, baselines, and
  cleanup behavior before implementing the command.
- Make compatibility policy explicit: preserve aliases, deprecate old syntax,
  or document a deliberate breaking change.

## Review checklist

Before implementation, verify:

1. The command grammar identifies the action, resource type, and resource ID
   without relying on mutually exclusive flags.
2. Flags are orthogonal and their defaults/cardinality are documented.
3. The smallest useful workflow has clear input and output artifacts.
4. State initialization, ownership, idempotency, and failure behavior are
   explicit.
5. Human output and machine-relevant exit codes are deterministic.
6. Tests cover the happy path, invalid combinations, conflicts, repeated
   selectors, and compatibility behavior.

## Example Workflow

1. Identify the smallest useful command set.
2. Define the input and output artifacts for each command.
3. Implement the core processing pipeline behind the CLI.
4. Add tests for the command and pipeline behavior.
5. Defer dashboards or interactive surfaces until the CLI is stable.

## Acceptance Criteria

The output is acceptable when it:

- defines a minimal useful CLI
- keeps the execution path understandable
- avoids premature UI or orchestration layers
- supports incremental extension later
- has a command grammar that can grow without accumulating flag aliases
- distinguishes scaffolding, installation, tracking, and adoption workflows
- documents side effects and validates conflicts before writes
