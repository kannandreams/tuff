---
title: The tuff.toml File
description: What tuff.toml is, when a capability needs one, and what goes in it.
---

`tuff.toml` is the manifest file for a capability. It sits in the capability's
folder and declares the capability's id, type, version, description, and files,
plus any settings specific to its type.

```text
security-review/
├── tuff.toml     ← the manifest
└── index.js      ← the capability's files
```

Tuff reads `tuff.toml` when you install the capability. The file is not copied
into agent folders such as `.agents/` or `.claude/`.

`tuff.toml` describes one capability and is written by the capability's author.
The record of what is installed in your project is a separate file,
[`tuff.lock`](/concepts/lockfile), which Tuff writes.

## Do I need one?

It depends on the type of capability. A skill's `SKILL.md` already contains
what Tuff needs. Every other type has settings that only `tuff.toml` can hold,
such as a tool's parameters or an MCP server's launch command.

| Capability | `tuff.toml` |
|---|---|
| [Skill](/primitives/skills) | Optional. `SKILL.md` is enough on its own |
| [Tool](/primitives/tools) | Required. It declares the parameters and how to run the tool |
| [MCP server](/primitives/mcp-servers) | Required. It declares how to start or reach the server |
| [Hook](/primitives/hooks) | Required for a Tuff-standard hook. A native hook added with `--hook-file` does not need one |
| [Policy](/primitives/policies) | Required. It holds the rules |

Without a `tuff.toml`, Tuff works out the type from the command or the folder:

- `tuff add skill <source>` names the type directly.
- `tuff add <path>` looks at the parent folder: `skills/` or `skill/` means a skill, `tools/` or `tool/` means a tool.

## The four fields every tuff.toml has

```toml title="tuff.toml"
id = "release-checklist"
type = "skill"
version = "1.0.0"
description = "Steps to follow before tagging a release."
```

| Field | What it means | Required |
|---|---|---|
| `id` | The capability's name. It becomes the folder it installs into, such as `.agents/skills/release-checklist/` | Yes |
| `type` | One of `skill`, `tool`, `mcp-server`, `hook`, or `policy` | Yes |
| `version` | The capability's version, such as `1.0.0` | Yes |
| `description` | One sentence saying what it does. For a tool, the agent reads this to decide when to call it | Yes |
| `files` | The files that make up the capability, relative to `tuff.toml` | No |

The `version` here is what the author says it is. When you install from Git
at a tagged release, the tag's version is recorded instead; see
[Install a release](/cli/add/#install-a-release).

## The extra section for each type

Apart from skills, each type adds one section of its own:

| Type | Section | Details |
|---|---|---|
| `skill` | None | [Skills](/primitives/skills) |
| `tool` | `[parameters]` and `[implementation]` | [Tools](/primitives/tools#manifest) |
| `mcp-server` | `[server]` | [MCP Servers](/primitives/mcp-servers#manifest) |
| `hook` | `[hook]` | See below and [Hooks](/primitives/hooks) |
| `policy` | `[[policy.rules]]` | [Policies](/primitives/policies#format) |

For example, a tool declares what input it takes and what to run:

```toml title="tuff.toml"
id = "security-review"
type = "tool"
version = "1.0.0"
description = "Scan a directory for common security vulnerabilities."
files = ["index.js"]

[parameters]
type = "object"
required = ["target_dir"]

[parameters.properties.target_dir]
type = "string"
description = "Directory to scan"

[implementation]
language = "node"
entrypoint = "index.js"
```

A Tuff-standard hook declares when to run and what command to run:

```toml title="tuff.toml"
id = "pre-commit-lint"
type = "hook"
version = "1.0.0"
description = "Check formatting before the agent finishes."

[hook]
event = "before_finish"
command = "cargo fmt --check"
```

| `[hook]` field | What it means | Required |
|---|---|---|
| `event` | When the hook runs, such as `session_start` or `before_finish`. The full list is in the [Hooks Specification](/spec/hooks/) | Yes |
| `command` | The command to run | Yes |
| `working_directory` | Where the command runs. Defaults to `.` | No |

Tuff translates the event into each agent's own hook settings, and refuses the
install if an agent cannot run that event.

## What Tuff refuses

Capabilities are often installed from other people's repositories, so Tuff
checks two things before installing: the capability's `id`, and the file paths
it lists. If either check fails, Tuff stops with an error and installs nothing.

Installing never runs any of the capability's code. Tuff only copies files.

### The id must be a simple name

This rule is about the value written in the `id` field, not about folder names
in a repository:

```toml title="tuff.toml"
id = "release-checklist"
```

Tuff uses this value to build two paths: the folder it installs the capability
into, and the folder `tuff delete` removes. An id such as `release-checklist`
installs into `.agents/skills/release-checklist/`.

Any ordinary name passes, so most authors never see this check. It exists for
a `tuff.toml` or `tuff.lock` that has been edited by mistake or on purpose to
point somewhere else. For example, `id = "../../../home"` would make Tuff write
or delete files outside your project. Tuff refuses an id like that before doing
anything.

Allowed:

- any ordinary name, such as `release-checklist` or `python-3.12`
- names joined by `/`, to group capabilities into a subfolder, such as `security/security-review`

Refused:

- an empty id
- `..` or `.` used as a name, such as `../release-checklist`
- a `/` at the start or end, or two in a row, such as `/release-checklist` or `security//review`
- a backslash, such as `security\review`
- a space at the start or end

The same rule applies wherever Tuff reads a capability name: the `--name`
option, the capabilities inside a pack, and the entries in `tuff.lock`.

### Listed files must be inside the capability's folder

Every path in `files`, and a tool's `entrypoint`, is relative to the folder
that holds `tuff.toml`, and must point to a file inside that folder.

| Example | Result | Why |
|---|---|---|
| `index.js` | Allowed | |
| `scripts/run.sh` | Allowed | A subfolder of the capability |
| `../shared/run.sh` | Refused | Points outside the capability's folder |
| `/usr/local/bin/run.sh` | Refused | Absolute paths are not allowed |
| A symbolic link | Refused | A link could point anywhere on the machine |
| A folder | Refused | List the files inside it instead |
| A file that does not exist | Refused | Every listed file must be present |

Two more things to know about `files`:

- **A leading `src/` is removed on install.** `src/check.sh` installs as `check.sh`. Because of that, listing both `check.sh` and `src/check.sh` is refused: both would be written to the same place.
- **A tool's `entrypoint` is included automatically.** You do not need to repeat it in `files`.

## Finding a capability in a Git repository

When you install from Git, the name you give is the capability's folder in the
repository:

```sh frame="terminal"
tuff add skill https://github.com/owner/repo rust-implement
```

Tuff looks for that folder at `skills/rust-implement`, `skill/rust-implement`,
`tools/rust-implement`, `tool/rust-implement`, and at the repository root.

For a local source, `--name` does something different: it renames the
installed capability, overriding the `id`.

## What gets installed

For a skill with `id = "python-uv-default"`, Tuff writes:

```text
.agents/skills/python-uv-default/SKILL.md
```

and adds an entry for it to [`tuff.lock`](/concepts/lockfile). `tuff.toml`
itself is not installed.

Files that already live in your project, such as `scripts/deploy.sh`, can be
tracked where they are without copying. See
[Adding Existing Agent Files](/cli/add/#adding-existing-agent-files).

## Where to keep capability sources

Tuff does not ship capabilities itself. Keep their sources:

- in the project that uses them, at any path, or
- in a separate repository that a person, team, or company maintains, installed from Git or bundled as a [pack](/concepts/packs/).

The Tuff repository has runnable examples under `examples/`, such as
`examples/tools/` and `examples/mcp-servers/`.
