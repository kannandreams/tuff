---
title: Packs in Your Application
description: Install, review, update, and remove capability packs and Claude Code plugins from your own application, using Tuff's JSON output.
---

This guide is for developers of an application that gives agents capabilities on behalf of its users: an agent platform, an internal tool, or anything with a "plugins" page. It shows how to let a user install a pack or a Claude Code plugin from a git address, review what it adds before anything is written, keep your own settings with it, and preview and apply updates. Every step is one `tuff` command with `--json`. A Rust application can call the [`tuff-packs` library](/guides/packs-from-rust/) for inspecting, importing, building, and checking packs.

:::note[Tuff version]
These commands need Tuff 0.18.0 or later. Check with `tuff --version`, and see [Installation](/installation/) to upgrade.
:::

## How your application calls Tuff

- Run `tuff` as a subprocess, with the project folder as its working directory, and read standard output.
- Every document described here carries `"schema": 1`. Fields can be added without changing that number, so ignore fields you do not know. The [Pack JSON Specification](/spec/packs/) lists every field.
- With `--json`, a failure is one line of JSON on standard error, and the exit code says what kind of failure it was:

```json
{"error":{"kind":"refused","message":"pack acme/review is already installed","hint":"use 'tuff update <member>' to move it to another release"}}
```

| Exit code | Meaning |
|---|---|
| `0` | Success |
| `1` | The operation failed: not found, refused, local changes, or a source was unreachable |
| `2` | The command was called wrongly |
| `70` | A bug in Tuff |

`kind` is one of `usage`, `not_found`, `refused`, `drift`, `source`, `corrupt`, `unsupported`, `io`, or `internal`. Show `message` to your user, and `hint` when there is one.

Tuff never prompts in these commands, and it never runs a pack's code while inspecting, installing, updating, or removing it.

## Set up a project

Tuff installs into a project: a folder with a `tuff.lock`, which records what is installed, from where, and the hashes that detect edits. Give each agent workspace its own folder and run once:

```sh frame="terminal"
tuff init
```

`init` writes `tuff.lock`, `tuff.config.json`, and the `tuff-cli-guide` skill for the default harness. Choose the harness your agents read with `-a` on each install: `open-agents` (the `.agents/` layout that Codex, Cursor, GitHub Copilot, Gemini CLI, Pi, and others read), `claude`, `codex`, `cursor`, `opencode`, or `pi`. A pack lists the harnesses it is built for in `[build] targets`, and installing for one outside that list is refused.

## 1. Show what a pack adds

Before installing, inspect the pack where it lives:

```sh frame="terminal"
tuff pack inspect https://github.com/acme/review-pack --json
tuff pack inspect https://github.com/acme/plugins --plugin review-kit --json
tuff pack inspect https://github.com/acme/agents --path packs/review --ref v1.2.0 --json
```

The repository may hold a `tuff-pack.toml` or a Claude Code plugin or marketplace. Tuff checks it out in a temporary folder, builds it, and prints one document. Nothing in the project changes. A shortened example:

```json
{
  "schema": 1,
  "name": "acme/review",
  "version": "1.2.0",
  "description": "Review capabilities.",
  "author": { "name": "Acme", "url": "https://example.com" },
  "source": {
    "kind": "git",
    "url": "https://github.com/acme/review-pack",
    "path": "",
    "ref": "4b1e9c2f0d7a3e5b8c6f1a2d9e0b7c4f5a6d3e1b",
    "tag": "v1.2.0"
  },
  "targets": ["claude", "open-agents"],
  "members": [
    {
      "id": "github",
      "type": "mcp-server",
      "version": "2.0.0",
      "runsCode": true,
      "runsCodeReason": "the harness starts the MCP server command as a local process",
      "files": { "claude": [".claude/mcp-servers/github/server.toml"] },
      "sharedFiles": { "claude": [".mcp.json"] },
      "mcp": {
        "transport": "stdio",
        "command": "npx",
        "args": ["-y", "@modelcontextprotocol/server-github"],
        "env": ["GITHUB_TOKEN"],
        "headerEnv": []
      },
      "x": { "myapp": { "default_level": "ask" } }
    },
    {
      "id": "review",
      "type": "skill",
      "version": "1.2.0",
      "runsCode": false,
      "runsCodeReason": "instructions and files the agent reads; the harness runs nothing from a skill on its own",
      "files": { "claude": [".claude/skills/review/SKILL.md", ".claude/skills/review/scripts/check.sh"] },
      "sharedFiles": { "claude": [] },
      "x": {}
    }
  ],
  "unsupported": [],
  "x": { "myapp": { "default_level": "read" } }
}
```

What to show a user before they install:

- **What runs code.** `runsCode` is `true` for a hook (the harness runs it on an event), a tool (the agent runs its entrypoint), and an MCP server over stdio (the harness starts a local process). An HTTP MCP server and a skill are `false`. A skill's `files` can include scripts the agent may choose to run, so list them.
- **What it connects to and needs.** `mcp` gives each server's command or URL, and `env` and `headerEnv` name the environment variables it reads. Values are never in a pack: a manifest can only reference a variable, so your application provides secrets in the agent's environment.
- **What it writes.** `files` and `sharedFiles`, per harness.
- **What was left out.** For a Claude Code plugin, `unsupported` lists each part Tuff does not carry, with `kind`, `name`, `path`, and `reason`, for example a slash command or a hook.
- **Exactly which release.** `source.ref` is the full commit, and `source.tag` the release tag that chose it.

## 2. Install what was reviewed

Install with the tag the inspection resolved, then confirm the commit:

```sh frame="terminal"
tuff add pack https://github.com/acme/review-pack --ref v1.2.0 -a claude --json
```

```json
{
  "schema": 1,
  "name": "acme/review",
  "version": "1.2.0",
  "digest": "9f2c…",
  "source": { "kind": "git", "url": "https://github.com/acme/review-pack", "path": "", "ref": "4b1e9c2f…", "tag": "v1.2.0" },
  "targets": ["claude"],
  "members": [
    { "id": "github", "type": "mcp-server", "version": "2.0.0" },
    { "id": "review", "type": "skill", "version": "1.2.0" }
  ],
  "unsupported": []
}
```

- If `source.ref` differs from the commit the user reviewed, the tag moved in between. Remove the pack (`tuff pack remove acme/review`) and show the new inspection.
- Pass `--ref <tag>` so updates follow release tags. `--ref <commit>` also installs exactly that commit, but the pack then has nothing to follow, and `tuff update` refuses until it is installed from a tag or a branch.
- The install is all or nothing: if a member is already tracked, or a file it writes already exists and Tuff did not write it, nothing is installed.
- Pass `--plugin <name>` and `--path <dir>` as for `inspect`. A marketplace entry installs only when its plugin is in the marketplace's own repository; for an entry that names another repository, install from that repository.

## 3. Keep your own settings with a pack

A pack author can put settings for your application in the pack, under a namespace you publish, such as your product's name:

```toml title="tuff-pack.toml"
[x.myapp]
default_level = "read"
```

The same works in each member's `tuff.toml`. Tuff checks only that the namespace is made of letters, digits, `-`, and `_`. It keeps the table byte for byte, returns it as `x` in the inspect document and in `tuff pack list --json`, records the pack's table in `tuff.lock`, and never reads a key inside it. What a key means, and what your application does when it is missing, is yours to document.

## 4. List and remove installed packs

```sh frame="terminal"
tuff pack list --json
```

```json
{
  "schema": 1,
  "packs": [
    {
      "name": "acme/review",
      "version": "1.2.0",
      "digest": "9f2c…",
      "source": { "kind": "git", "url": "https://github.com/acme/review-pack", "path": "", "ref": "4b1e9c2f…", "tag": "v1.2.0" },
      "members": [
        { "id": "github", "type": "mcp-server", "version": "2.0.0", "targets": ["claude"] },
        { "id": "review", "type": "skill", "version": "1.2.0", "targets": ["claude"] }
      ],
      "x": { "myapp": { "default_level": "read" } }
    }
  ]
}
```

```sh frame="terminal"
tuff pack remove acme/review
```

`remove` deletes every member as one change, including its hook registrations and MCP entries. It refuses with kind `drift` when the user edited a member's files; `--force` removes them anyway.

## 5. Check for updates

```sh frame="terminal"
tuff outdated --json
```

`outdated` prints one row per member per harness. For a pack from git, `latest` is the newest release tag's version, or the branch's newest commit for a pack that follows a branch:

| `status` | Meaning |
|---|---|
| `up to date` | The installed release is the newest |
| `outdated` | A newer release is tagged, or the branch moved |
| `repointed` | The installed tag now names a different commit |
| `tag missing` | The installed tag no longer exists |
| `not checked` | Nothing to compare against, such as a pack installed from a commit |
| `error` | The repository could not be reached |

Members of one pack share a row status, so group rows by pack using `tuff pack list --json`.

## 6. Preview and apply an update

Name any member to preview its whole pack:

```sh frame="terminal"
tuff update review --check --json
```

```json
{
  "schema": 1,
  "pack": "acme/review",
  "status": "update",
  "from": { "version": "1.2.0", "digest": "9f2c…", "source": { "kind": "git", "tag": "v1.2.0", "…": "…" } },
  "to": { "version": "1.3.0", "digest": "1d07…", "source": { "kind": "git", "tag": "v1.3.0", "…": "…" } },
  "targets": ["claude"],
  "members": [
    {
      "id": "review",
      "change": "updated",
      "fromVersion": "1.2.0",
      "toVersion": "1.3.0",
      "files": [
        {
          "target": "claude",
          "path": ".claude/skills/review/SKILL.md",
          "status": "modified",
          "patch": "diff --git a/SKILL.md b/SKILL.md\n…"
        }
      ]
    },
    { "id": "github", "change": "unchanged", "fromVersion": "2.0.0", "toVersion": "2.0.0", "files": [] }
  ],
  "blockedBy": []
}
```

- `status` is `update` when there is a release to apply, `current` when there is none, and `repointed` when the installed tag names a different commit. The last two carry a `message` and no members.
- `change` is `added`, `updated`, `removed`, or `unchanged`. `patch` is a unified diff, empty for a binary file. Files are compared with what is on disk now.
- `blockedBy` lists members the user edited, as `id (harness)`. Applying the update then needs `--force`, which replaces their edits.

Apply it, with the same member:

```sh frame="terminal"
tuff update review
```

The update checks out the new release, builds it, and replaces the old one as one change. A Claude Code plugin is imported again from the new release. Run the inspection of step 1 against the new tag first if your application reviews every release.

## Claude Code plugins

Every command above works on a repository that holds a Claude Code plugin (`.claude-plugin/plugin.json`) or a marketplace (`.claude-plugin/marketplace.json`). Tuff reads the plugin as a pack each time:

- skills become skill members, and MCP servers in `.mcp.json` or `plugin.json` become MCP server members, with `${VAR}` values as references;
- commands, agents, hooks, output styles, LSP servers, programs in `bin/`, and MCP servers whose values are written into the config or that run a program bundled in the plugin are listed under `unsupported` and not installed;
- the pack is built for every harness that can write all of its members, so the same plugin's skills and servers can reach Codex, Cursor, or OpenCode as well as Claude Code.

To keep a plugin as a pack you can edit, run [`tuff pack import`](/cli/packs/#import-a-claude-code-plugin). To publish your own packs to Claude Code users, build them as a [plugin or a marketplace](/cli/packs/#build-a-claude-code-plugin).

## Related

- [Packs from Rust](/guides/packs-from-rust/): the same inspect, import, build, and update checks from the `tuff-packs` crate.
- [Pack JSON Specification](/spec/packs/): every field of every document on this page, with a JSON Schema.
- [Packs CLI reference](/cli/packs/): every flag of `tuff pack` and `tuff add pack`.
- [Capability Packs](/concepts/packs/): what a pack is and how its artifact is verified.
