# Changelog

All notable changes to the Tuff VS Code extension are documented here. The
extension versions independently of the Tuff CLI; the CLI's own changelog
lives at https://tuffcli.dev/changelog/.

## [Unreleased]

## [0.1.0] - 2026-09-05

First release.

### Added

- A capabilities tree, grouped by kind, listing every installed skill, tool,
  hook, workflow, and MCP server with its recorded version, the agents it was
  installed for, and whether the installed files still match what was
  recorded. A capability installed for several agents folds into one row that
  expands.
- A status bar summary of capabilities that are modified, missing, repointed,
  or outdated. It says nothing about updates until they have been checked,
  rather than implying everything is current.
- Check for Updates, which runs `tuff outdated` and annotates rows with the
  move available and the claimed size of the change. It is a command rather
  than an automatic refresh because it reaches the network and clones git
  sources; `tuff.checkUpdatesOnStartup` opts into running it when a workspace
  opens.
- Validate Capabilities, which runs `tuff check` and reports what no longer
  matches its recorded state.
- Per-row commands: show local changes, show upstream changes, update, and
  reveal on disk. Diffs open as a real diff document. Clicking a capability
  opens its entry file, or reveals its directory when it has none.
- Run MCP Doctor, which spawns each installed server and reports the
  handshake and tool list.
- Settings for the executable path, the scope the tree lists, and the startup
  update check.

Requires Tuff 0.6.0 or newer, which is the release that made `list`,
`outdated`, and `diff` machine-readable. The extension bundles no binary and
runs the `tuff` already on the machine.
