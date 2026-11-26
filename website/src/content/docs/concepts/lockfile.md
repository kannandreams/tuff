---
title: The tuff.lock File
description: What tuff.lock records about each installed capability, which commands change it, why to commit it, and how older lockfiles migrate.
---

`tuff.lock` is the record of the capabilities installed in your project. Tuff
creates it at the project root when you run `tuff init`, and updates it every
time you install, update, or remove a capability. Do not edit it by hand.

For each installed capability, `tuff.lock` records:

- **What was installed:** its name, type, and version.
- **Where it came from:** a local path, a Git repository and commit, the MCP catalog, or a pack.
- **Where it went:** the agent it was installed for and the folder its files were written to.
- **A hash of the installed files**, which Tuff uses to detect later edits.

[`tuff.toml`](/primitives/format) describes one capability and is written by
the capability's author. `tuff.lock` describes your project and is written by Tuff.

## Commit it to Git

Commit `tuff.lock` with the rest of your project. Your teammates and CI then
work from the same record, and [`tuff check`](/cli/ci/#tuff-check) fails when
installed files no longer match it.

Tuff uses these files:

| File | What it holds | Commit to Git? |
|---|---|---|
| `tuff.lock` | The installed capabilities | Yes |
| `tuff.config.json` | The registered agents and the default agent. See [below](#tuffconfigjson) | Usually yes |
| Tuff's user config and state folders | Global preferences, and the lockfile for [global scope](/concepts/scopes) | No |
| Tuff's user cache folder | Cached copies of installed files, used as the baseline for detecting edits | No |

The cache is safe to delete at any time, for example with `tuff cache clear`.
Tuff rebuilds it by fetching and verifying the sources recorded in `tuff.lock`.

## Which commands change it

| Command | Changes `tuff.lock` | What happens |
|---|---|---|
| `tuff init` | Yes | Creates the file |
| `tuff create` | Yes | Adds an entry for the new capability |
| `tuff add` | Yes | Adds an entry for each installed capability |
| `tuff scan --adopt` | Yes | Adds an entry for each adopted capability |
| `tuff update` | Yes | Updates the capability's entry |
| `tuff delete` | Yes | Removes the capability's entry |
| `tuff untrack` | Yes | Removes the capability's entry |
| `tuff lock migrate` | Yes | Rewrites the file in the current format |
| `tuff scan` | No | Reads it to find untracked capabilities |
| `tuff list` | No | Reads it |
| `tuff status` | No | Reads it |
| `tuff outdated` | No | Reads it |
| `tuff diff` | No | Reads it |
| `tuff check` | No | Reads it |
| `tuff generate` | No | Reads it |
| `tuff harness` | No | Changes `tuff.config.json` only |

Commands that only read the file never rewrite it, so running them in CI
leaves the checkout clean.

## Example

```json title="tuff.lock"
{
  "version": 4,
  "capabilities": [
    {
      "name": "python-uv-default",
      "type": "skill",
      "version": "1.2.0",
      "version_scheme": "declared",
      "description": "Default Python project setup with uv.",
      "target": "open-agents",
      "installed_path": ".agents/skills/python-uv-default",
      "sha256": "...",
      "ownership": "generated",
      "source": {
        "kind": "local",
        "path": "examples/skills/python-uv-default"
      }
    }
  ]
}
```

`version` at the top is the version of the file format. Each item in
`capabilities` is one capability installed for one agent, so a skill installed
for both Claude Code and Open Agents has two entries. A project with installed
packs also has a `packs` object, described [below](#the-packs-object).

## Fields

| Field | What it means |
|---|---|
| `name` | The capability's id, such as `release-checklist` or `security/security-review` |
| `type` | `skill`, `tool`, `hook`, `mcp-server`, or `policy` |
| `version` | The capability's version |
| `version_scheme` | Where `version` came from: `semver` for a [release chosen by tag](/cli/add/#install-a-release), `declared` for the version written in `tuff.toml` or `SKILL.md`, or `sha` when there is no version and the commit is used instead |
| `description` | Copied from the manifest at install time |
| `target` | The agent the capability was installed for, such as `claude` or `open-agents` |
| `installed_path` | The folder the files were written to |
| `sha256` | A hash of the installed folder, used to detect edits |
| `ownership` | `generated` when Tuff wrote the files, or `imported` when Tuff tracks files that were already there |

A `name` that is not a simple relative path makes Tuff refuse the whole
lockfile as corrupt, because Tuff builds install and delete paths from it. The
error names the entry; remove that entry by hand. See
[What Tuff refuses](/primitives/format#what-tuff-refuses).

## The source object

`source` records where the capability came from. Its `kind` decides which
other fields it has:

| `kind` | Fields | Written by |
|---|---|---|
| `local` | `path`: the source folder, relative to the project when it is inside it. Empty for a capability adopted in place, whose only copy is the installed one | `tuff add <path>`, `tuff create` |
| `git` | `url`, `path` (the folder inside the repository), `ref` (the commit installed). When installed at a release, also `tag` (the tag chosen) and `requested` (the requirement you asked for, such as `^1.2`) | `tuff add <type> <url> <name>` |
| `catalog` | `id` (the catalog entry) and `version` (that entry's version at install) | `tuff add mcp <id>` |
| `pack` | `name`, `version` (the pack's release), `digest` (the pack artifact's hash), `path` (the capability's path inside the pack) | `tuff add pack` |

For a capability installed from a pack, the entry's own `version` is the
capability's version, and `source.version` is the pack's. Every capability from
the same pack shares this source, which is why `tuff update` on any one of them
updates the whole pack.

## The packs object

`packs` has one entry per installed pack, keyed by the pack's name. It records where the pack came from, so the pack can be listed, updated, and removed as one unit.

```json title="tuff.lock"
"packs": {
  "acme/review": {
    "version": "1.2.0",
    "digest": "9f2c...",
    "source": {
      "kind": "git",
      "url": "https://github.com/acme/review-pack",
      "path": "",
      "ref": "4b1e9c2...",
      "tag": "v1.2.0"
    },
    "members": [
      "code-review",
      "github"
    ]
  }
}
```

| `source.kind` | Fields | Written by |
|---|---|---|
| `artifact` | `registry`, when the artifact was installed with `--reference` | `tuff add pack <file>` |
| `git` | `url`, `path` (the folder holding `tuff-pack.toml`, empty for the root), `ref` (the commit installed), and `tag` or `branch` when one chose the commit | `tuff add pack <git-url>` |

`members` lists the capabilities whose `source` names the pack. Tuff writes it on every change; the capability entries are the record of membership. `version` and `digest` repeat what the members record, and a file where they disagree is refused as corrupt. For a pack from git, `digest` is the hash of the artifact Tuff built from the checkout, kept for reference.

## Extra fields for some types

- **Hooks and MCP servers** register entries in shared harness files, such as `.claude/settings.json` or `.mcp.json`. Their entries carry a copy of what Tuff registered, in `managed_hooks` or `managed_mcp_entry`, so an edit made by hand is detected.
- **Tools and MCP servers** keep a copy of their `implementation`, `parameters`, or `server` settings, so `tuff generate index` can describe them without the original `tuff.toml`.

## File format

- `tuff.lock` is JSON, file format version 4.
- Entries are sorted by name, type, agent, and installed path. A change touches only the lines for that capability, and merge conflicts stay local to it.
- The file uses two-space indentation, one array item per line, and a trailing newline. This is the layout `JSON.stringify(value, null, 2)` produces, and the one npm's `package-lock.json` uses.

Common formatters produce the same layout, so running them over the file
changes nothing: `jq`, Python's `json.dumps(indent=2)`, VS Code's JSON
formatter, and Prettier's `json-stringify` parser. The exception is Prettier's
default `json` parser, which puts short arrays on one line. If Prettier formats
`tuff.lock` in your repository, set its parser to `json-stringify` for this
file, the same way Prettier handles `package-lock.json`.

## Migrating from an older schema

| Format version | Syntax | Written by |
|---|---|---|
| 1 | TOML | Tuff 0.1.x |
| 2 | TOML | Tuff 0.2 to 0.7 |
| 3 | JSON | Tuff 0.8 to 0.17 |
| 4 | JSON | Tuff 0.18 and later |

Tuff reads all four versions, and every command works on any of them.

- Read-only commands leave an older file as it is.
- The first command that changes the lockfile rewrites it as version 4.
- To do the upgrade on its own, as a separate commit, run:

```sh frame="terminal"
tuff lock migrate
```

This rewrites the file as version 4 and changes nothing else. On a version 4
file it does nothing.

What the upgrade changes:

- **From version 3:** a project with installed packs gains the `packs` object, and each pack's `registry` moves from its members' `source` to the pack's entry. Nothing else changes.
- **From version 2:** only the syntax. The fields, their names, and their order stay the same.
- **From version 1:** the old columns are mapped to the new `source` object. `source = "git"` with `repository`, `source_path`, and `resolved_ref` becomes a `git` source. `source = "catalog"` becomes a `catalog` source. A row with a `pack` table becomes a `pack` source. Anything else becomes a `local` source. `version_scheme` is set to `sha` for Git entries and `declared` for the rest. The `emittedFiles` and `scope` fields are dropped, because the hash and the lockfile's location already hold that information.

:::caution[Everyone on the project needs Tuff 0.18 or newer]
Tuff 0.17 and earlier refuse a version 4 lockfile with an error asking to upgrade Tuff, and Tuff 0.7 and earlier cannot read version 3 or 4 at all. Once a project's lockfile is upgraded, everyone who runs Tuff on it, including through the VS Code extension, needs Tuff 0.18 or newer.
:::

Errors you may see:

- **A lockfile written by a newer Tuff** is refused with a message naming its version.
- **A file whose syntax does not match its version**, such as TOML that claims version 3 or JSON that claims version 2, is reported as corrupt, with a message saying which.

Versions 1, 2, and 3 remain readable. If a future release stops reading either,
the changelog will say so first. [Stability](/concepts/stability) states how the format changes after 1.0.

## tuff.config.json

`tuff.config.json` holds the project's agent settings. It is separate from
`tuff.lock`: it says which agents Tuff installs for, not what is installed.

```json title="tuff.config.json"
{
  "agents": ["open-agents", "claude"],
  "defaultAgent": "open-agents"
}
```

| Field | What it means |
|---|---|
| `agents` | The agents registered for this project |
| `defaultAgent` | The agent a command uses when you do not pass `-a/--harness` |

`tuff init` creates it. `tuff harness add <id>` and `tuff create` add agents,
`tuff harness set-default <id>` changes the default, and `tuff harness list` shows
both. Add `--global` to work with the global configuration instead.

Removing an agent with `tuff harness remove` only unregisters it. It does not
change `tuff.lock` or delete any files. To remove a capability's files, use
`tuff delete <id>`; to stop tracking a capability but keep its files, use
`tuff untrack <id>`. See [Harnesses and Scope](/cli/agents/).
