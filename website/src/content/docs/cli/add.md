---
title: Create and Add
description: Create new capabilities, install them from a local path or Git, and adopt the ones already on disk.
---

## `tuff create`

Create and track a new agent-local capability:

```sh frame="terminal"
tuff create skill my-skill
tuff create tool my-tool -a claude
tuff create hook review-hook -a open-agents -a claude
tuff create workflow release-flow -a claude
```

The capability type and id are positional. `-a, --harness` is optional and
repeatable. When omitted, Tuff uses the configured default agent. Creation
initializes Tuff state, registers the selected agents, writes adapter-valid
files, and records the baseline. Use `-a <agent>` when creating for a
different agent.

## `tuff add`

Install a capability from a local capability directory or Git URL. The command supports
two forms:

1. Let Tuff infer the capability type from a local path.
2. Use an explicit capability-type subcommand when the type is known or when
   installing from a Git repository.

The available capability types are `skill`, `tool`, `hook`, `workflow`, and
`mcp-server`. In the examples below, `<capability-type>` means “replace this
placeholder with one of those types.” External MCP servers have their own
subcommand, [`tuff add mcp`](/cli/mcp/#install-an-mcp-server), and packs have
[`tuff add pack`](/cli/packs/#install-a-pack).

### Local sources

For a local capability directory, Tuff can infer the type from its location or
from its manifest:

```sh frame="terminal"
# Auto-detect the type
tuff add ./my-skill

# Explicit type
tuff add <capability-type> ./path/to/capability

# Explicit type with a selected agent
tuff add <capability-type> ./path/to/capability -a claude

# Multiple agents
tuff add <capability-type> ./path/to/capability -a claude -a open-agents

# Global scope
tuff add <capability-type> ./path/to/capability --global
```

For typed capability commands, options such as `--harness` and `--global` come
after the source:

```sh frame="terminal"
tuff add tool ./my-tool -a claude --global
```

Add existing agent files in place without copying their content:

```sh frame="terminal"
tuff add -a open-agents .agents/skills/my-skill
tuff add -a claude .agents/skills/my-skill
```

Override the installed ID for a local source with `--name`; the overridden ID is used for emitted paths and the lockfile entry:

```sh frame="terminal"
tuff add ./path/to/capability --name team-capability -a open-agents
```

### Git sources

```sh frame="terminal"
tuff add <capability-type> https://github.com/owner/repo <name> -a open-agents
tuff add <capability-type> https://github.com/owner/repo <name> -a claude
```

For Git sources, `<name>` is the capability directory name inside the
repository. For example:

```sh frame="terminal"
tuff add skill https://github.com/owner/repo rust-implement -a open-agents
```

Use the same structure for a tool, hook, or workflow by replacing `skill` with
the corresponding capability type.

### Install a release

If a repository tags its releases, you can install a specific one. Add `@` and a version after the name:

```sh frame="terminal"
# Exactly this release
tuff add skill https://github.com/owner/repo rust-implement@1.2.0 -a open-agents

# The newest release in a range
tuff add skill https://github.com/owner/repo rust-implement@^1.2 -a open-agents
```

Tuff reads the repository's tags (without downloading the repository), picks the newest release that matches what you asked for, and then downloads the repository at that tag.

**Which tags count as releases.** A tag counts when it is a version number:

| Tag | Used when |
|---|---|
| `v1.4.0` or `1.4.0` | The repository holds one capability |
| `rust-implement/v1.4.0` or `rust-implement-v1.4.0` | The repository holds several capabilities, so each tag names the one it releases |

If any tag names the capability, Tuff looks only at those tags. That way a
repository-wide `v2.0.0` is never mistaken for a release of one skill inside it.

**What you can ask for.**

| You write | Tuff installs |
|---|---|
| `@1.2.0` | Exactly 1.2.0 |
| `@^1.2` | The newest 1.x that is 1.2.0 or later |
| `@~1.4` | The newest 1.4.x |
| `@1` | The newest 1.x |
| `"@>=1, <2"` | The newest release from 1.0.0 up to, but not including, 2.0.0 (quote it in the shell) |

Prereleases such as `2.0.0-rc.1` are never picked from a range. To install one, ask for it by its exact version.

**What gets recorded.** The lockfile still pins the exact commit. Next to it,
the entry records the tag that was chosen and the requirement you asked for,
and the entry's version becomes the tag's version:

```json
{
  "name": "rust-implement",
  "version": "1.4.0",
  "version_scheme": "semver",
  "source": {
    "kind": "git",
    "url": "https://github.com/owner/repo",
    "path": "rust-implement",
    "ref": "9b9c499…",
    "tag": "v1.4.0",
    "requested": "^1.2"
  }
}
```

**When nothing matches.** If no release satisfies the requirement, the install
fails before anything is downloaded and lists the releases that do exist. A
repository with no release tags fails the same way, with a hint on how to tag one.

To move an installed release forward later, see [`tuff update`](/cli/diff-update/#tuff-update).

### Declared versions

Without `@`, a git install takes the latest commit. Its recorded version is then whatever the source declares for itself, checked in this order:

1. `version` in `tuff.toml`
2. `version:` or `metadata.version:` in the `SKILL.md` frontmatter, which is where the Agent Skills specification puts it

The lockfile marks that with `"version_scheme": "declared"`. A source that declares nothing records the commit SHA as its version, with `"version_scheme": "sha"`.

A declared version is what the author wrote, not a release: it may not change when the content does. So for a git install, `tuff list` and `tuff outdated` show it as `1.2.0 (declared)`, visibly weaker than a release chosen by tag. `tuff outdated` can report the row `outdated` while `LATEST` still reads `1.2.0`, meaning the commit moved and the version did not. A local install's version is declared by definition and is shown plain.

### Hooks from harness settings

For harness-native hooks, pass the hook fragment explicitly:

```sh frame="terminal"
tuff add hook ./claude-session-start -a claude --hook-file settings.json
```

Tuff-standard manifest hooks are validated against adapter compatibility and rendered to the
target harness's native settings. To inspect hook support or check a tracked hook before switching
adapters:

```sh frame="terminal"
tuff hooks matrix
tuff hooks check-portability pre-commit-lint --target claude
```

### Flags

| Flag | Description |
|---|---|
| `-a, --harness <id>` | Harness to install for (optional, repeatable; defaults to the configured harness). `--agent` is the older name |
| `-g, --global` | Install to global user scope |
| `-n, --name <id>` | Override the installed capability ID for an auto-detected local source |
| `--hook-file <path>` | Hook-only native settings fragment, relative to the hook source directory |
| `--accept-unenforced` | For a policy added with `tuff add <path>`: install the rules each agent enforces and record the rest in `tuff.lock`, instead of refusing the policy. See [Rules an agent does not enforce](/primitives/policies/#rules-an-agent-does-not-enforce) |

The capability type is specified as a subcommand (`skill`, `tool`, `hook`, or
`workflow`) rather than a `--type` flag. For a typed local source, the name is
optional and is normally inferred from the source. For a Git source, the name
is required so Tuff knows which capability directory to discover.

## `tuff scan`

Find capabilities already sitting in a harness folder that Tuff is not
tracking. `list`, `status`, and `check` all read the lockfile, so anything
you wrote by hand in `.claude/skills/` is invisible to them; `scan` is the
discovery half that `tuff add` has always needed.

It reads `.claude`, `.cursor`, and `.agents`, and it changes nothing:

```sh frame="terminal"
tuff scan
```

```text
┌────────────────────────────────────┬──────────────────────┬───────┬─────────┬─────────────┬─────────────┐
│ PATH                               │ ID                   │ TYPE  │ VERSION │ AGENT       │ STATUS      │
├────────────────────────────────────┼──────────────────────┼───────┼─────────┼─────────────┼─────────────┤
│ .agents/skills/rust-best-practices │ rust-best-practices  │ skill │ 1.1.0   │ open-agents │ + untracked │
│ .claude/hooks/session-start        │ session-start        │ hook  │ —       │ claude      │ · blocked   │
│ .claude/skills/find-skills         │ find-skills          │ skill │ —       │ claude      │ + untracked │
└────────────────────────────────────┴──────────────────────┴───────┴─────────┴─────────────┴─────────────┘
.claude/hooks/session-start is missing the [hook] section in tuff.toml; adopt a native hook with 'tuff add hook <path> --hook-file <fragment>'
2 untracked; track them with 'tuff scan --adopt'
```

| Status | Meaning |
|---|---|
| `untracked` | Ready to track. `--adopt` takes these. |
| `tracked` | Already in the lockfile at this path. |
| `conflict` | Two directories declare the same id, so one lockfile key would have to hold both paths. Adopt one explicitly with `tuff add <path> --name <name>`. |
| `blocked` | Missing something Tuff needs before it can track it at all, usually a `[hook]`, `[server]`, or `[implementation]` section in a `tuff.toml`. The reason is printed below the table. |

Track what it found:

```sh frame="terminal"
tuff scan --adopt                          # every untracked capability
tuff scan --adopt .claude/skills/find-skills  # just this one
```

Adoption is [in place](#adding-existing-agent-files): nothing is moved or
copied, and the lockfile records the path the capability already has.

Scanning works before `tuff init` has run, which is how you can look before
you commit to anything. Tracking does not: it writes to `tuff.lock`, so
`--adopt` asks you to run `tuff init` first.

:::caution[Adopted capabilities have no upstream]
A capability Tuff adopts was already on your disk, so there is nowhere to
check for a newer version. `tuff outdated` will keep reporting it as
unchecked. `diff`, `check`, and `update` still work: they compare against
the baseline recorded at adoption.
:::

`--json` emits one object per row, with `status`, `reason`, and an
`initialized` flag saying whether this folder has a lockfile yet.

## Adding Existing Agent Files

`tuff scan` finds these for you. To bring one under Tuff management
directly, when you already know its path:

```sh frame="terminal"
tuff add -a open-agents .agents/skills/python-uv
```

### Before/after

```text
Before add:
.agents/skills/python-uv/
  └── SKILL.md                ← existing, unmanaged

After tuff add -a open-agents .agents/skills/python-uv:
.agents/skills/python-uv/
  └── SKILL.md                ← untouched
tuff.config.json
  ├── tuff.lock              ← entry added
  └── objects/
    └── sha256/
      └── a1/
        └── b2c3...           ← immutable baseline object
```

After add, the directory participates in the full lifecycle. `tuff list`,
`tuff diff`, `tuff check`, and `tuff update` all work without modifying
your existing agent files. Use `tuff update <id>` to accept intentional local
edits as the new baseline.

:::tip[After add]
When Tuff adopts existing agent files, it records their hashes as baselines in the lockfile. No additional files are created in the agent directory. The lockfile is the single source of truth for all tracking metadata.
:::
