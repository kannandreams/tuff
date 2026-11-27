---
title: Packs
description: Build, publish, install, and update capability packs.
---

New to packs? Start with the [Tuff Pack examples repository](https://github.com/kannandreams/tuff-pack-examples). It follows tracked capabilities through build, inspection, GHCR publication, pull, extraction, and a container image before introducing the lower-level details. See [Capability Packs](/concepts/packs/) for the source manifest and artifact guarantees.

## `tuff pack`

### Build

Package all project-scoped tracked capabilities except `tuff-cli-guide`, using version `0.1.0` and the configured default agent:

```sh frame="terminal"
tuff pack build --name crm-integration
```

The default output is `tuff-dist/crm-integration-0.1.0.tuffpack`. Select capabilities, targets, a version, or a different output when needed:

```sh frame="terminal"
tuff pack build --name crm-integration --version 1.2.0 \
  --capability crm-operating --capability lead-triage \
  -a open-agents -a claude \
  --output releases/crm-integration-1.2.0.tuffpack
```

Create a reusable project-backed definition under `tuff-packs/<name>/tuff-pack.toml` without copying capability files:

```sh frame="terminal"
tuff pack init crm-integration --from-project \
  --capability crm-operating --capability lead-triage
tuff pack check tuff-packs/crm-integration
tuff pack build tuff-packs/crm-integration
```

Project builds fail when selected capability files or sources differ from the accepted `tuff.lock` baseline; accept intentional changes with `tuff update <capability>` first.

Standalone path-based source packs remain supported. Both commands default to the current directory:

```sh frame="terminal"
tuff pack check [path]
tuff pack build [path] --output <artifact.tuffpack>
```

For a standalone pack, omitting `--output` writes `<pack-name>-<pack-version>.tuffpack` beneath the pack root. Build always refuses to overwrite an existing artifact.

### Inspect and verify

```sh frame="terminal"
tuff pack inspect <artifact.tuffpack>
tuff pack inspect ./review-pack --json
tuff pack inspect https://github.com/acme/review-pack --ref v1.2.0 --json
tuff pack verify <artifact.tuffpack>
```

`pack inspect` reads an artifact, a source pack folder, or a git URL, with `--ref` and `--path` as for [`tuff add pack`](#install-from-git). A folder or repository is built in a temporary directory, so nothing in the project changes. The text form lists each member and marks the ones that run code. `--json` prints one document with the pack's metadata, its source (for git, the resolved commit), and for each member the files it writes on each harness, whether it runs code (hooks, tools, and MCP servers started as local processes), the MCP servers it starts or connects to with the environment variables they read, and its host tables. The [Pack JSON Specification](/spec/packs/) describes every field.

To review a pack from git and then install exactly what was reviewed, pass the `source.ref` from the inspect document to `tuff add pack <url> --ref <commit>`.

### Push and pull

Publish a verified artifact to an OCI registry, or pull it back by an explicit tag or digest:

```sh frame="terminal"
tuff pack push <artifact.tuffpack> ghcr.io/yourorg/crm-integration:1.2.0
tuff pack pull ghcr.io/yourorg/crm-integration:1.2.0 --output crm-integration-1.2.0.tuffpack
tuff pack pull ghcr.io/yourorg/crm-integration@sha256:<manifest-digest> --output pinned.tuffpack
```

- `pack push` refuses to move a tag that already names different content unless `--force` is supplied. Pushing identical content is idempotent.
- `pack pull` resolves a tag to an immutable manifest digest before downloading, verifies the OCI descriptors and the complete Tuff artifact, and refuses to overwrite an existing output file.
- Both commands accept `--json`, repeatable `--ca-file <pem>`, and `--plain-http` for disposable development registries.
- Tuff uses existing Docker credentials first, then Podman credentials, and otherwise attempts anonymous access.

### Extract

Extract one pre-rendered adapter target without creating project lockfile state:

```sh frame="terminal"
tuff pack extract <artifact.tuffpack> -a <id> --output <directory>
```

The output directory must be missing or empty.

## Install a pack

Install every member of a verified local pack artifact into project scope:

```sh frame="terminal"
tuff init
tuff add pack ./tuff-dist/crm-integration-1.2.0.tuffpack -a open-agents
```

Pack installation verifies the complete artifact, preflights every member, stages shared hook and MCP configuration, and refuses the entire installation if any member is already tracked or would overwrite an untracked file. `--harness` is optional and repeatable; pack installation does not support `--global`.

If the pack came from a registry, pass `--reference` with the reference you
pulled it from so `tuff outdated` can check for a newer version later:

```sh frame="terminal"
tuff pack pull ghcr.io/acme/engineering:1.2.0 --output ./engineering.tuffpack
tuff add pack ./engineering.tuffpack -a open-agents \
  --reference ghcr.io/acme/engineering:1.2.0
```

`tuff add pack` only ever sees the local artifact file; it has no way to know
where it came from unless told. Without `--reference`, `tuff outdated` reports
this pack's capabilities as `not checked` rather than guessing.

### Install from git

A repository that holds a standalone `tuff-pack.toml` installs from its address:

```sh frame="terminal"
tuff add pack https://github.com/acme/engineering-pack -a open-agents
tuff add pack https://github.com/acme/agents --path packs/engineering
tuff add pack https://github.com/acme/agents/tree/main/packs/engineering
tuff add pack https://github.com/acme/engineering-pack --ref v1.2.0
```

- Without `--ref`, Tuff installs the newest release tag. Release tags are `v1.2.0` or `1.2.0`, or, in a repository with several packs, `<name>/v1.2.0` or `<name>-v1.2.0`, where `<name>` is the last part of the pack name (`engineering` for `com.acme/engineering`). When any tag names the pack, only those tags count.
- A repository with no release tags installs its default branch, and later updates follow that branch.
- `--ref` takes a tag, a branch, or a commit.
- A release tag names a version, so the `tuff-pack.toml` at that tag must declare the same version, or the install is refused.
- Each harness you install for must be listed in the pack's `[build] targets`.
- A pack whose `tuff-pack.toml` selects capabilities from its author's project (`[project]`) cannot be installed from git, because only its author can build it.

Tuff checks out the commit, builds the pack, and installs it with the same checks as an artifact. Nothing in the repository runs: Tuff does not fetch submodules and runs no hooks. `tuff.lock` records the repository, the folder, the commit, and the tag or branch that chose it. `--json` prints the installed pack, its source, and its members.

### Install a Claude Code plugin from git

A repository whose folder holds a Claude Code plugin (`.claude-plugin/plugin.json`) or a marketplace (`.claude-plugin/marketplace.json`) installs the same way. Tuff reads the plugin as a pack, as [`tuff pack import`](#import-a-claude-code-plugin) does, and installs its skills and MCP servers:

```sh frame="terminal"
tuff add pack https://github.com/acme/review-kit -a claude -a codex
tuff add pack https://github.com/acme/plugins --plugin review-kit -a claude
```

- Release tags are read against the plugin's name, and a plugin that declares a `version` must declare the tag's version.
- `--plugin` picks one entry of a marketplace. The entry must be in the marketplace's own repository, so that updates follow that repository's tags; for an entry that names another repository, install from that repository, or import it with `tuff pack import --plugin`.
- What the plugin has that Tuff does not carry is printed after the install and returned under `unsupported` with `--json`. Nothing from it is written into the project.
- `tuff update` imports the new release the same way.

## Import a Claude Code plugin

`tuff pack import` reads a plugin, or one plugin of a marketplace, and writes it as a standalone source pack:

```sh frame="terminal"
tuff pack import ./review-kit --output packs/review-kit
tuff pack import https://github.com/acme/plugins --plugin review-kit --output packs/review-kit
tuff pack import ./review-kit --output packs/review-kit --json
```

| In the plugin | In the pack |
|---|---|
| `skills/<name>/` and folders named by `skills` in `plugin.json` | A skill member per folder, with every file in it |
| A `SKILL.md` at the plugin's root, when it has no `skills/` | One skill named after the plugin |
| Servers in `.mcp.json` and in `mcpServers` in `plugin.json` | An `mcp-server` member per server |
| `name`, `version`, `description`, `author`, `homepage`, `license`, `keywords` | The pack's manifest |
| Everything else | Listed under `unsupported` in `tuff-pack.toml`, and kept under `extra/` |

An MCP server becomes a member when Tuff can write it for every harness exactly as Claude Code would run it:

- an environment value or header must be a reference such as `${GITHUB_TOKEN}` or `Bearer ${API_KEY}`. A value written into the config is not carried, since it may be a secret. A `${VAR:-default}` default is dropped, with a note;
- a server whose command runs a program bundled in the plugin (`${CLAUDE_PLUGIN_ROOT}`) is not carried, since Tuff does not install the plugin's own files for other harnesses;
- the `stdio` and `http` transports are carried; `sse` and `ws` are not.

The pack's `[build] targets` are every harness that can write all of its members. A harness that cannot is left out with a note saying why: Pi takes no skills, and Codex passes an environment variable to a server only under its own name, so a server that reads `GITHUB_TOKEN` as `GITHUB_PERSONAL_ACCESS_TOKEN` keeps the pack off Codex.

Commands, agents, hooks, output styles, LSP servers, programs in `bin/`, `userConfig`, and dependencies are not carried. Hooks are left out because a Claude Code hook can have a matcher and handler types that a Tuff hook cannot express. Each item names its reason. The output folder must be missing or empty, and `tuff-pack.toml` records where the plugin came from and the commit. `--json` prints the [inspect document](/spec/packs/) of the imported pack.

## List and remove packs

```sh frame="terminal"
tuff pack list
tuff pack list --json
tuff pack remove com.acme/engineering
```

`tuff pack list` shows each installed pack with its version, where it came from, and its members. `tuff pack remove` deletes every member as one change, including hook registrations and MCP entries, and refuses when a member has local edits unless `--force` is given.

## Update a pack

A capability installed by `tuff add pack` moves forward with its pack, never on its own. Naming any member updates every member:

- capabilities the new release drops are removed,
- new ones are installed,
- the rest are replaced, with shared hook and MCP registrations adjusted to match.

The pack is the unit of versioning and verification, so a lockfile never records two releases of one pack.

```sh frame="terminal"
# Resolve the registry recorded by `tuff add pack --reference`, pull the
# newest semver tag, and apply it
tuff update <member-id>

# Preview: the version, what would be added, updated, or removed, and
# whether local edits stand in the way
tuff update <member-id> --check

# The same preview as JSON, with the files that change
tuff update <member-id> --check --json

# Apply a pulled artifact instead of resolving the registry (offline, or a
# pack installed without --reference)
tuff update <member-id> --pack ./engineering-1.2.0.tuffpack

# Development registries, as for `tuff outdated`
tuff update <member-id> --plain-http --ca-file ./registry-ca.pem
```

A pack update applies to every agent the pack is installed for; a narrower `--harness` selection is refused rather than leaving one agent on the old release. Local edits to any member block the update unless `--force` is given. Only semver tags are compared when resolving the registry, matching `tuff outdated`; when nothing parses, pass `--pack` with the artifact you mean.

A pack installed from git updates from its repository. One installed at a release tag moves to the newest release tag, and one installed from a branch moves to the branch's newest commit, even when the version is unchanged. A pack installed from a bare commit has nothing to follow; reinstall it with `--ref <tag>`, or pass `--pack`. When the installed tag now names a different commit, `tuff outdated` reports `repointed`, and `tuff update` refuses without `--force`.

### Preview an update as JSON

`tuff update <member-id> --check --json` prints the preview without changing anything:

```json
{
  "schema": 1,
  "pack": "com.acme/engineering",
  "status": "update",
  "from": { "version": "1.1.0", "digest": "...", "source": { "kind": "git", "...": "..." } },
  "to": { "version": "1.3.0", "digest": "...", "source": { "kind": "git", "tag": "v1.3.0", "...": "..." } },
  "targets": ["open-agents"],
  "members": [
    {
      "id": "pack-skill",
      "change": "updated",
      "fromVersion": "1.1.0",
      "toVersion": "1.3.0",
      "files": [
        {
          "target": "open-agents",
          "path": ".agents/skills/pack-skill/SKILL.md",
          "status": "modified",
          "patch": "diff --git a/SKILL.md b/SKILL.md\n..."
        }
      ]
    }
  ],
  "blockedBy": []
}
```

- `status` is `update` when there is a release to apply, `current` when the pack is up to date, and `repointed` when the installed tag names a different commit. The last two carry a `message` and no members.
- `change` is `added`, `updated`, `removed`, or `unchanged`. Files are compared with what is on disk now, and `patch` is empty for a binary file.
- `blockedBy` lists members with local edits, as `id (harness)`. Applying the update needs `--force` while it is not empty.
- For a pack from a registry, the JSON preview pulls the new release to list its files; the text preview does not pull.
