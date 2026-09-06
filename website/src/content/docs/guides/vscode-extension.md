---
title: VS Code Extension
description: See installed capabilities, drift, and available updates in the editor sidebar, and act on them without leaving it.
---

Tuff ships a VS Code extension. It puts the capabilities installed in a project into the sidebar, shows which of them have drifted or fallen behind, and runs the lifecycle commands on a row.

The extension contains no binary. It runs the `tuff` you have already installed, the same arrangement the [Claude Code plugin](/guides/claude-code-plugin/) uses, so there is one Tuff on the machine and one place versions come from.

## Install

Search for **Tuff** in the Extensions view, or install it from the command line:

```sh frame="terminal"
code --install-extension kannandreams.tuff
```

It requires Tuff 0.6.0 or newer, the release that made `list`, `outdated`, and `diff` machine-readable. Check what you have with `tuff --version`, and see [Installation](/installation/) if the command is missing. When `tuff` is not on the editor's `PATH`, set `tuff.path` to its full path.

## Cursor, and other editors

Cursor installs VS Code extensions, so this one works there, but Cursor searches [Open VSX](https://open-vsx.org) rather than the Visual Studio Marketplace and the extension is not on Open VSX yet. Until it is, install the `.vsix` by hand: download it from the [Marketplace page](https://marketplace.visualstudio.com/items?itemName=kannandreams.tuff) under **Download Extension**, then use **Extensions: Install from VSIX** in the command palette, or `cursor --install-extension tuff-0.1.0.vsix`. The same applies to any other editor that reads Open VSX.

Claude Code and Codex sessions running inside VS Code operate on the same project directory, so anything they install appears in the tree on the next refresh.

### Build it from source

Contributing, or trying an unreleased change:

```sh frame="terminal"
git clone https://github.com/kannandreams/tuff.git
cd tuff/editors/vscode
npm install
npm run package
code --install-extension tuff-0.1.0.vsix
```

`npm run package` writes a `.vsix` next to the manifest; the version in the filename follows `package.json`.

## A walkthrough for the first run

**Help: Welcome**, then **Get Started with Tuff**, walks through install, initialize, scan, and the catalog, running each command from the page and ticking a step off once it has run. VS Code features it in any workspace holding a `tuff.lock` or a `.claude`, `.cursor`, or `.agents` folder.

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
| Browse MCP Catalog | `tuff mcp catalog --json`, then `tuff add mcp <id> --yes` |
| Scan for Existing Capabilities | `tuff scan --json`, then `tuff scan --adopt <paths>` |
| Initialize Project | `tuff init` |
| Add from Git URL | `tuff add <url> --name <name>` |

The four row commands appear on a capability's context menu. Acting on a capability row applies to every agent it is installed for; acting on an agent row narrows to that one, exactly as passing `--agent` does. Diffs open as a real diff document rather than as plain text.

Deleting and packing stay in the CLI. The extension is a view with a few safe actions on top of it, not a replacement for it.

### Scan for Existing Capabilities

Most projects that install this extension already have skills in them, written by hand or dropped in from somewhere else, and Tuff knows about none of them. **Tuff: Scan for Existing Capabilities** reads `.claude`, `.cursor`, and `.agents`, lists what it finds, and lets you pick what to track.

Everything picked is tracked [where it already is](/cli/#tuff-scan). Nothing is moved, copied, or rewritten: the lockfile records the path the capability already has, and the files are left alone.

The picker offers only what Tuff can actually take. A directory that shares an id with another one, or that is missing a `[hook]` or `[server]` section it needs, is reported with the reason instead — the same statuses [`tuff scan`](/cli/#tuff-scan) prints.

Scanning changes nothing and works in a folder Tuff has never seen, so the empty view offers it alongside **Initialize Project**. If you scan first and pick something in a folder that has no `tuff.lock`, the extension says so and offers to run `tuff init` before tracking, since tracking is what needs the lockfile.

This command needs Tuff 0.7.0 or newer, the release that added `tuff scan`.

### Add from Git URL

Paste a repository, or a directory inside one, and the extension runs `tuff add <url> --name <name>` for the harnesses the project is configured for. That covers a skills.sh link, which points at a directory in a repository: the name is prefilled from the URL's last segment, so the common case is one Enter. Add `@1.2.0` to the name to pin a release, exactly as on the command line.

The name is asked for every time. The CLI only needs it when the source has no `tuff.toml`, but knowing that would mean cloning first, and when a manifest is present the name you give simply wins. A local path is turned away with a pointer to Scan, which is what handles a directory already on disk.

### Browse MCP Catalog

The one install the extension does offer is from the [built-in catalog](/mcp-catalog/), because the catalog is a fixed, curated list rather than an arbitrary source. Pick a server and Tuff wires it into the harnesses this project is configured for.

An entry that needs an API key asks you to confirm first, and names the variables. No key passes through the editor: Tuff records the variable name as a `{ from_env = "NAME" }` reference, so exporting it stays your job and the value never reaches the lockfile or the manifest.

Install runs with `--yes`, which accepts the catalog's own variable names. Renaming one is an interactive prompt, so do that from the CLI with `tuff add mcp <id>`.

This command needs Tuff 0.7.0 or newer, the release that added `tuff mcp catalog`. On an older CLI the extension says so, and the rest of the view keeps working.

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
