# Pack JSON Specification

Version 1. This document describes the JSON documents Tuff prints about capability packs, so that an application which installs packs for its own users can show what a pack adds before installing it, preview an update, and keep its own settings in a pack. It also describes host tables, the part of a pack manifest Tuff carries for such an application. The [JSON Schema for the inspect document](https://github.com/kannandreams/tuff/blob/main/spec/packs/inspect.schema.json) and [an example of it](https://github.com/kannandreams/tuff/blob/main/spec/packs/inspect.example.json) sit beside this file. The example is what `tuff pack inspect --json` prints for the [example pack](https://github.com/kannandreams/tuff/tree/main/spec/packs/example/review-pack), and `mise run check` fails when either drifts from the code. On tuffcli.dev the two files are served at `/spec/packs/inspect.schema.json` and `/spec/packs/inspect.example.json`.

The words MUST, SHOULD, and MAY are used as in RFC 2119. For a walk through these documents in the order an application uses them, see [Packs in Your Application](https://tuffcli.dev/guides/packs-in-your-application/).

## 1. Scope

A pack is a named, versioned set of capabilities: skills, tools, hooks, and MCP servers. See [Capability Packs](https://tuffcli.dev/concepts/packs/) for the source format and the artifact. A **host** is an application that runs agents and gives them packs, by calling `tuff` with `--json` or by linking Tuff's crates. This specification covers:

- host tables in `tuff-pack.toml` and `tuff.toml` (section 2);
- the inspect document, printed by `tuff pack inspect --json` (section 3);
- the install document, printed by `tuff add pack --json` (section 4);
- the list document, printed by `tuff pack list --json` (section 5);
- the update preview, printed by `tuff update <member> --check --json` (section 6).

## 2. Host tables

A pack manifest (`tuff-pack.toml`) and a capability manifest (`tuff.toml`) MAY contain any number of tables under `x`, each named by a namespace the host chooses:

```toml
[x.example-host]
default_level = "read"
screens = [{ resource = "ui://changelog/preview", may_call = ["list_pull_requests"] }]
```

- A namespace is one or more ASCII letters, digits, `-`, or `_`. Tuff refuses any other namespace, and refuses an `x` entry that is not a table.
- Tuff never reads a key inside a host table and never acts on one. A host SHOULD use a namespace unlikely to collide with another host's, such as its product name.
- A built artifact stores the pack's `tuff-pack.toml` and every member's `tuff.toml` as written, so host tables, comments, and formatting survive build, verify, extract, and install byte for byte.
- JSON documents carry host tables as JSON objects converted from TOML. A TOML date or time becomes its RFC 3339 string.
- An installed pack's host tables are recorded in `tuff.lock`, on the pack's entry in `packs`, so the list document returns them without the pack's source.

## 3. The inspect document

`tuff pack inspect <source> --json` reads a pack from a `.tuffpack` artifact, a source folder (or its `tuff-pack.toml`), or a git URL with optional `--ref` and `--path`. A source folder or repository is built in a temporary directory for every target its manifest declares, so the document says what each member writes. Nothing in the current project changes.

| Field | Type | Meaning |
|---|---|---|
| `schema` | number | `1` |
| `name`, `version`, `description` | string | From `tuff-pack.toml` |
| `homepage`, `license` | string, optional | From `tuff-pack.toml` |
| `keywords` | array of string | From `tuff-pack.toml`, empty when none |
| `author` | object, optional | `name`, and optionally `email` and `url` |
| `source` | object | Where the pack was read from: `{ "kind": "artifact", "path" }`, `{ "kind": "folder", "path" }`, or `{ "kind": "git", "url", "path", "ref", "tag"?, "branch"? }`, where `ref` is the full commit |
| `digest` | string | SHA-256 of the artifact bytes, lowercase hex. For a folder or repository, the artifact Tuff built, which a different Tuff release can render differently |
| `artifactVersion` | number | The artifact format version |
| `targets` | array of string | Target ids, sorted |
| `members` | array | One object per member, sorted by id (below) |
| `import` | object, optional | For a pack imported from another bundle format: `format` (such as `claude-plugin`), and where it was read from: `url`, `path`, `plugin` (the marketplace entry), `commit` |
| `unsupported` | array | Parts of the source Tuff could not carry, each with `kind` (such as `commands`, `hooks`, or `mcp-server`), optional `name` and `path`, and `reason`. Empty for a pack written as a Tuff pack |
| `x` | object | The pack's host tables |

Each member:

| Field | Type | Meaning |
|---|---|---|
| `id`, `type`, `version`, `description` | string | From the member's `tuff.toml`. `type` is `skill`, `tool`, `hook`, `mcp-server`, or `policy` |
| `sourcePath` | string | The member's folder in the source pack |
| `runsCode` | boolean | Whether the harness or the agent runs code from this member (section 3.1) |
| `runsCodeReason` | string | One sentence saying why |
| `files` | object | Per target id, the project-relative files the member writes |
| `sharedFiles` | object | Per target id, the shared harness files the member adds an entry to, such as `.claude/settings.json` for a hook or `.mcp.json` for an MCP server |
| `mcp` | object, optional | For an MCP server, or a tool served over MCP: `transport` (`stdio` or `http`), `command`, `args`, `url`, `env` (the names of the environment variables it reads), and `headerEnv` (the variables HTTP headers are read from). Values never appear, since a manifest holds only references |
| `tool` | object, optional | For a tool: `language`, `entrypoint`, `mcp`, `runtimeDeps` |
| `hook` | object, optional | For a hook: `event`, `command`, `workingDirectory` |
| `x` | object | The member's host tables |

### 3.1 What runs code

| Member | `runsCode` | Why |
|---|---|---|
| hook | `true` | The harness runs the hook command when its event fires |
| tool | `true` | The agent runs the tool's entrypoint |
| MCP server over stdio | `true` | The harness starts the server command as a local process |
| MCP server over HTTP | `false` | The harness connects to a remote server |
| skill | `false` | Instructions and files the agent reads |

A skill's folder can contain scripts the agent may choose to run. `files` lists every file a skill writes, so a host can show them; `runsCode: false` covers only what runs without the agent choosing to.

## 4. The install document

`tuff add pack <source> --json` prints, after a successful install:

| Field | Meaning |
|---|---|
| `schema` | `1` |
| `name`, `version`, `digest` | The installed pack |
| `source` | `{ "kind": "artifact", "registry"? }` or `{ "kind": "git", "url", "path", "ref", "tag"?, "branch"? }` |
| `targets` | The harnesses installed for |
| `members` | `id`, `type`, `version` for each member |
| `unsupported` | For a pack imported from another bundle format, what it had that the pack does not carry, as in section 3 |

For a Claude Code plugin installed from git, `source` also has `format: "claude-plugin"`, and `plugin` when a marketplace entry was chosen.

A host that showed the inspect document for a git URL SHOULD install with `--ref` set to that document's `source.ref`, so that what is installed is the commit that was reviewed.

## 5. The list document

`tuff pack list --json` prints `{ "schema": 1, "packs": [...] }`. Each pack has `name`, `version`, `digest`, `source` (as in section 4), `members` (`id`, `type`, `version`, `targets`), and `x` when the pack has host tables.

## 6. The update preview

`tuff update <member> --check --json` changes nothing and prints:

| Field | Meaning |
|---|---|
| `schema` | `1` |
| `pack` | The pack's name |
| `status` | `update` when there is a release to apply, `current` when the pack is up to date, `repointed` when the installed tag names a different commit or artifact. `current` and `repointed` carry a `message`, `to: null`, and no members |
| `from`, `to` | `version` and `digest` of the installed and the new release, and for an update the `source` of each |
| `targets` | The harnesses the pack is installed for |
| `members` | `id`, `change` (`added`, `updated`, `removed`, `unchanged`), `fromVersion`, `toVersion`, and `files`: `target`, `path`, `status` (`added`, `removed`, `modified`), and `patch`, a unified diff of the file that is empty for a binary file |
| `blockedBy` | Members with local edits, as `id (target)`. Applying needs `--force` while this is not empty |

Files are compared with what is on disk in the project now.

## 7. Versioning

Each document carries `schema`. A field MAY be added to a document without changing its schema number, so a reader MUST ignore fields it does not know. Removing a field, renaming one, or changing what one means is a new schema number, and the changelog says so first.
