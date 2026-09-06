# Changelog

All notable changes to the Tuff VS Code extension are documented here. The
extension versions independently of the Tuff CLI; the CLI's own changelog
lives at https://tuffcli.dev/changelog/.

## [Unreleased]

### Added

- **Scan for Existing Capabilities**, on the view menu, in the command palette, and offered from the empty view. It reads `.claude`, `.cursor`, and `.agents` through `tuff scan --json`, lists the capabilities Tuff is not tracking, and tracks the ones you pick where they already sit: nothing is moved, copied, or rewritten. Directories Tuff cannot take — two sharing an id, or one missing a `[hook]` or `[server]` section — are reported with the reason rather than offered. Scanning works in a folder that is not a Tuff project yet; picking something there offers **Initialize Project** first, since only tracking needs a lockfile. Needs Tuff 0.7.0 or newer.
- **Initialize Project**, which runs `tuff init` and then offers to scan.
- **Browse MCP Catalog**, on the view menu and in the command palette. It lists the servers compiled into your `tuff` binary, with what each one runs and the variables it expects, and installs the one you pick for the harnesses the project is configured for. An entry that needs an API key names its variables and asks before installing; Tuff records the variable name as a reference, so no key reaches the editor, the lockfile, or the manifest. The list comes from `tuff mcp catalog --json`, so the extension carries no copy of the catalog and cannot offer a server the CLI would refuse. Needs Tuff 0.7.0 or newer; on an older CLI the command says so and the rest of the view is unaffected.

### Fixed

- A folder with no `tuff.lock` no longer reads as an empty Tuff project. The
  view asked `tuff list`, which succeeds with an empty result whether or not
  the project was ever initialized, so the welcome content offering
  `tuff init` could never appear. It now looks for the lockfile itself. A
  CLI command that runs and fails no longer flips the view to "no project"
  either, since a failed command says nothing about what is on disk.
- `vsce package` now compiles before packaging. Without the
  `vscode:prepublish` hook, the vsix shipped whatever `out/` happened to
  hold, which need not have matched `src/`.

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
