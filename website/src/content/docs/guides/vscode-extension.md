---
title: VS Code Extension
description: See installed capabilities, drift, and available updates in the editor sidebar, and act on them without leaving it.
---

Tuff ships a VS Code extension. It puts the capabilities installed in a project into the sidebar, shows which of them have drifted or fallen behind, and runs the lifecycle commands on a row.

The extension contains no binary. It runs the `tuff` you have already installed, the same arrangement the [Claude Code plugin](/guides/claude-code-plugin/) uses, so there is one Tuff on the machine and one place versions come from.

## Install

Search for **Tuff** in the Extensions view, or install it from the command line:

```sh
code --install-extension kannandreams.tuff
```

It requires Tuff 0.6.0 or newer, the release that made `list`, `outdated`, and `diff` machine-readable. Check what you have with `tuff --version`, and see [Installation](/installation/) if the command is missing. When `tuff` is not on the editor's `PATH`, set `tuff.path` to its full path.

## Works in more than VS Code

Cursor installs VS Code extensions, so the same extension covers it, and it is published to [Open VSX](https://open-vsx.org) for editors that read that registry. Claude Code and Codex sessions running inside VS Code operate on the same project directory, so anything they install appears in the tree on the next refresh.

## What it shows

The **Capabilities** view groups everything installed in the project by kind: skills, tools, hooks, workflows, and MCP servers. Each row carries the version recorded in the lockfile and the agents the capability was installed for. A capability installed for several harnesses is one row that expands into one child per agent.

Rows carry the same drift states [`tuff list`](/cli/#tuff-list) reports. A capability whose installed files no longer match what was recorded reads as modified; one whose files are gone reads as missing. The status bar carries the counts, so a hand edit is visible before an agent session runs into it.

Clicking a capability opens its entry file, `SKILL.md` or `server.toml` or `tuff.toml`, and reveals the directory in the Explorer when it has none.

## Updates are asked for, not assumed

[`tuff outdated`](/cli/#tuff-outdated) reaches the network and clones git sources to answer, which is not something a sidebar should do every time a file is saved. So the extension does not check for updates on its own. Run **Tuff: Check for Updates** from the view title or the command palette, and rows gain the move available and the claimed size of the change, such as `1.2.0 to 1.4.0 (minor)`.

Until that has run, the view says `updates not checked` rather than showing everything as current. A release tag that moved or vanished upstream is shown as its own finding, not as staleness, matching how the CLI reports it.

## Commands

| Command | What it runs |
|---|---|
| Refresh Capabilities | `tuff list --json` |
| Check for Updates | `tuff outdated --json` |
| Validate Capabilities | `tuff check --json` |
| Show Local Changes | `tuff diff <id>` |
| Show Upstream Changes | `tuff diff <id> --upstream` |
| Update Capability | `tuff update <id>` |
| Run MCP Doctor | `tuff mcp doctor` |

The four row commands appear on a capability's context menu. Acting on a capability row applies to every agent it is installed for; acting on an agent row narrows to that one, exactly as passing `--agent` does. Diffs open as a real diff document rather than as plain text.

Installing, deleting, and packing stay in the CLI. The extension is a view with a few safe actions on top of it, not a replacement for it.

## Settings

| Setting | Default | What it does |
|---|---|---|
| `tuff.path` | `tuff` | Path to the executable. |
| `tuff.scope` | `all` | Which scope the tree lists: `all`, `project`, or `global`. |
| `tuff.checkUpdatesOnStartup` | `false` | Check for updates when a workspace opens. Off because it reaches the network. |

## Not in this version

Drift does not appear in the Problems panel. `tuff check` reports drift per capability rather than per file, so a diagnostic would have nothing accurate to point at. Reporting which files drifted is a CLI change that has to come first.

## Source

The extension lives in the Tuff repository under `editors/vscode`, and versions independently of the CLI. Issues belong on the [main tracker](https://github.com/kannandreams/tuff/issues).
