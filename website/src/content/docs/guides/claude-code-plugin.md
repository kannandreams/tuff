---
title: Claude Code Plugin
description: Install Tuff as a Claude Code plugin so an agent session can install, diff, and validate capabilities without leaving the conversation.
---

Tuff ships a Claude Code plugin. Installing it puts the Tuff command reference in front of the agent, so you can ask for a capability change in the session instead of switching to a terminal and describing the result afterwards.

The plugin does not contain the Tuff binary. It teaches the agent to use a `tuff` you have already installed, and tells it how to install one if the command is missing.

## Install

Add the marketplace, then install the plugin:

```sh
claude plugin marketplace add kannandreams/tuff
claude plugin install tuff@tuff
```

Inside a session, the same two steps are `/plugin marketplace add kannandreams/tuff` and `/plugin install tuff@tuff`. Add `--scope project` to the install command to record it in `.claude/settings.json` so everyone working in the repository gets it.

Install the CLI itself from the [installation page](/installation/) if `tuff --version` does not answer.

## What you get

One skill, `tuff-cli-guide`. It is the same reference `tuff init` writes into `.agents/skills/tuff-cli-guide/`, covering installs, packs, inspection, updates, CI validation, the directory model, and drift status values.

The difference is where it comes from. `tuff init` gives the guide to one project. The plugin gives it to every session on the machine, including projects that have not run `tuff init` yet, which is exactly the case where an agent most needs to be told that `tuff init` is the first step.

Both copies are generated from a single file in the repository, so they never disagree about what a command does.

## What it looks like in use

Once installed, requests like these route through Tuff rather than through hand-copied files:

- "Install the rust-implement skill from pproenca/dot-skills for Claude Code."
- "Wire up the GitHub MCP server for this repo."
- "Has anything drifted from its baseline since last week?"
- "Check the capabilities the way CI would."

The agent runs the real commands. The results land in `tuff.lock` and in the harness directories, reviewable in a diff like any other change.

## Other harnesses

Cursor and Codex have no equivalent plugin surface yet. Until they do, `tuff init` installs the same guide into `.agents/skills/`, which every harness Tuff supports reads. The plugin is an additional distribution channel for Claude Code, not a different capability set.

## For maintainers

The plugin lives in `plugins/tuff/` and the marketplace manifest is `.claude-plugin/marketplace.json` at the repository root. The skill file is generated: edit `crates/tuff-cli/assets/tuff-cli-guide.md`, then run `mise run plugin-sync`. The repository check runs `mise run plugin-check`, which fails when the generated copy drifts and, where the `claude` binary is available, validates both manifests in strict mode.
