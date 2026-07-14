---
title: Usage Scenarios
description: How teams can adopt and use Coral in practice.
---

Coral is for teams that want agent capabilities to be managed like normal
engineering assets: visible in a project, reviewable in pull requests, and
reproducible across machines.

## Team capability pack

A platform or developer-experience team maintains a pack repository:

```text
company-agent-pack/
  coral-pack.toml
  capabilities/
    rust-test-workflow/
    security-review/
    release-prep/
```

Product teams install the capabilities they need:

```sh frame="terminal"
coral init
coral add https://github.com/company/company-agent-pack --capability rust-test-workflow
coral list
```

The project owns the installed output. If a team customizes it, Coral should
show that drift instead of hiding it.

## Project-specific capabilities

Some agent behavior belongs to a single project. For example:

- how to run that service locally
- how to test a migration
- how to review domain-specific code
- how to prepare a release

Those capabilities can live inside the project and still use Coral for
validation, install state, drift detection, and future merge behavior.

## Personal or global setup

An engineer may also keep personal capabilities in a global location and load
them into a harness. Coral should support that flexibility later, but the
strongest team workflow is project-owned state that can be reviewed and shared.

Global capabilities are useful for personal preferences. Project capabilities
are better for team conventions.

## Adopting external skills

Teams should be able to adopt useful skills from ecosystems such as skills.sh
or GitHub repositories, then bring them under Coral lifecycle tracking.

The intended flow is:

```sh frame="terminal"
coral import https://github.com/owner/repo --skill rust-implement
coral list
coral diff rust-implement
```

Import and remote-source behavior is still roadmap work, but it is an important
adoption path because many teams already have copied or externally installed
agent assets.

## Harness compilation

Different coding agents expect different file layouts. Coral should keep a
single managed source model and compile or emit target-specific output:

```sh frame="terminal"
coral target add open-agents
coral target add claude
```

Harness adapters make target output explicit and reproducible. The same
managed capability can be emitted into `.agents/` for Open Agents-compatible
harnesses or `.claude/` for Claude Code.
