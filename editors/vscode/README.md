# Tuff for VS Code

See the agent capabilities installed in a project, and act on them, without
leaving the editor.

Tuff manages the skills, tools, hooks, workflows, and MCP servers a coding
agent reads from a repository. This extension is a view onto that: it runs
the `tuff` CLI and renders what it reports.

## What it does

- **A capabilities tree**, grouped by kind, showing every installed skill,
  tool, hook, workflow, and MCP server, the version recorded for it, and the
  agents it was installed for.
- **Drift at a glance.** A capability edited by hand since it was installed
  shows as modified; one whose files are gone shows as missing. The status
  bar carries the same counts, so a hand edit is visible before an agent
  session runs into it.
- **Updates, when you ask for them.** Checking for updates reaches the
  network and clones git sources, so it is a command rather than something
  the view does on its own. Once run, rows show the move available, the
  claimed size of the change, and any release tag that moved or vanished
  upstream.
- **Commands on a row.** Show local changes, show upstream changes, update,
  and reveal on disk. Clicking a capability opens its entry file.
- **MCP doctor**, which spawns each installed server and reports what it
  actually answers.

## Requirements

The extension does not bundle a binary. It runs the `tuff` you have
installed, so there is one Tuff on the machine and one place versions come
from. Install it first:

```sh
uv tool install tuffcli      # or: brew install kannandreams/tuff/tuff
tuff --version
```

The [installation guide](https://tuffcli.dev/installation/) covers the other
channels. Version 0.6.0 or newer is required, since the extension reads the
`--json` output that release added to `list`, `outdated`, and `diff`.

If `tuff` is not on the editor's `PATH`, set `tuff.path` to its full path.

## Works in

VS Code, and editors that install VS Code extensions, including Cursor.
Claude Code and Codex sessions running inside VS Code see the same project,
so the tree reflects whatever those sessions install.

## Settings

| Setting | Default | What it does |
|---|---|---|
| `tuff.path` | `tuff` | Path to the executable. |
| `tuff.scope` | `all` | Which scope the tree lists: `all`, `project`, or `global`. |
| `tuff.checkUpdatesOnStartup` | `false` | Check for updates when a workspace opens. Off because it reaches the network. |

## Not in this version

- **Diagnostics in the Problems panel.** Drift is reported per capability
  rather than per file, so a squiggle would have nothing accurate to point
  at. It needs a CLI that reports which files drifted.
- **Editing capabilities.** Installing, deleting, and packing stay in the
  CLI. This is a view with a few safe actions, not a replacement for it.

## Links

- [Tuff documentation](https://tuffcli.dev)
- [CLI reference](https://tuffcli.dev/cli/)
- [Source and issues](https://github.com/kannandreams/tuff)

MIT licensed.
