---
title: Inspect and Generate
description: List installed capabilities, check for updates, and generate indexes and reports.
---

## `tuff list`

Show installed capabilities with scope, drift status, and path:

```sh frame="terminal"
tuff list
```

### Filters

```sh frame="terminal"
# By scope
tuff list --scope project
tuff list --scope global

# By capability type
tuff list --type skill
tuff list --type tool

# Combine filters
tuff list --scope global --type tool
```

### Status values

| Status | Meaning |
|---|---|
| `clean` | Installed content matches recorded hash |
| `modified` | Installed content has local changes |
| `missing` | Installed file no longer exists |

`tuff list` uses terminal colors when supported: clean is green, modified is amber, and missing is red.

### JSON

`tuff list --json` prints the same rows as an array, one object per capability per agent, with the keys `id`, `type`, `version`, `version_scheme`, `scope`, `target`, `status`, and `path`. An empty inventory prints `[]`.

`version_scheme` is one of:

- `semver`: a [release chosen by tag](/cli/add/#install-a-release)
- `declared`: a [version the source wrote for itself](/cli/add/#declared-versions)
- `sha`: a pinned commit

The `type`, `target`, and `status` keys are spelled the same as in `tuff check --json`, so one script can read both.

```sh frame="terminal"
tuff list --json | jq '.[] | select(.status != "clean") | .id'
```

## `tuff status`

Show per-primitive detail including scope, drift, and override warnings:

```sh frame="terminal"
tuff status
```

Example output:

```text
python-uv-default  project  clean  [overrides global: won't receive global updates]
commit-hygiene     global   clean
scan-tool          project  clean
```

## `tuff generate`

Generate derived Tuff artifacts from tracked project state:

```sh frame="terminal"
# Agent-facing capability index
tuff generate index -a open-agents
tuff generate index -a claude

# Custom index path
tuff generate index -a open-agents --output docs/CAPABILITIES.md

# Human-readable project report
tuff generate report
tuff generate report --output docs/tuff-report.md
```

`tuff generate index` writes the default index for the selected agent:

| Agent | Default output |
|---|---|
| `open-agents` | `.agents/CAPABILITIES.md` |
| `claude` | `.claude/CAPABILITIES.md` |

The generated index is intended for agent context. Point `AGENTS.md`,
`CLAUDE.md`, or equivalent agent instructions at the generated
`CAPABILITIES.md` file when you want the agent to see a compact inventory of
tracked capabilities.

`tuff generate report` writes `tuff-report.md` by default.
The report includes installed capabilities, agents, source type, emitted paths,
and clean/modified/missing status summaries.

Generated files are derived output. The source of truth is
`tuff.lock` tracking state.

## `tuff outdated`

Show all installed capabilities and whether upstream updates are available.
Read-only; never modifies files.

```sh frame="terminal"
tuff outdated
```

Example output:

```text
find-skills               skill      open-agents  2adcfe5    def5678    outdated
security-review           tool       claude       abc1234    2adcfe5    outdated
rust-implement            skill      open-agents  1.2.0      1.4.0      outdated (minor)
csv-workbench             skill      open-agents  1.0.0      —          not checked
crm-skill                 skill      open-agents  1.0.0      1.0.0      repointed
```

### Status values

| Status | Meaning |
|---|---|
| `up to date` | Nothing newer is available |
| `outdated` | A newer commit or release exists. For releases, it carries the claimed size of the change: `major`, `minor`, or `patch` |
| `repointed` | The installed tag or pack version now names different content than when you installed it |
| `tag missing` | The installed tag no longer exists upstream |
| `not checked` | Tuff has nowhere to check, so `LATEST` reads `—` |

`repointed` and `tag missing` win over `outdated`, because they change what the version you have means. `LATEST` still shows the newest release. Your install is unaffected either way: the lockfile pins the commit or digest, not the tag.

### Git capabilities that follow a commit

For a git capability installed without `@`, the row compares commits: HEAD moved or it did not.

- If the source declares no version, `CURRENT` and `LATEST` show the 7-character commit SHA.
- If it declares one, `CURRENT` shows it as `1.2.0 (declared)` and `LATEST` the version the source declares now, with the claimed size of the change when both parse.
- If the commit moved but the declared version did not, the row still reads `outdated` with `LATEST` equal to `CURRENT`.

The check costs one `ls-remote` and no clone. The one exception is a source that declares a version and whose commit moved: then Tuff clones to read the version it declares now.

That same listing shows whether the repository publishes releases. When it does, a note under the table suggests the `tuff update <id>@<requirement>` that pins the newest one. This is where Tuff tells you release tags exist: `tuff add` without an `@` never lists them, so an untagged install costs no extra round trip.

### Git capabilities installed at a release

For a capability installed at a [release](/cli/add/#install-a-release), `CURRENT` and `LATEST` are release versions.

- `outdated` carries the size of the change the version numbers claim: `major`, `minor`, or `patch`. That is the author's claim, not what the content shows; `tuff diff <id> --upstream` shows the content.
- `LATEST` is the newest release the repository has, whether or not your recorded requirement allows it. So an exact pin can read `outdated` while `tuff update` reports it up to date. `tuff update <id>@<requirement>` lifts the pin.
- Tuff also checks the installed tag still names the commit recorded at install. This costs nothing extra, since the one `ls-remote` already lists every tag with its commit.

### Pack capabilities

For a capability installed with [`tuff add pack --reference`](/cli/packs/#install-a-pack),
`CURRENT` and `LATEST` compare the *pack's* published versions (the numbers
you passed to `tuff pack build --version`), since the pack is what is actually being checked.

- The installed tag is resolved and its digest compared with the one recorded at install, which is how `repointed` and `tag missing` are found.
- This costs one small manifest fetch per pack per run, not per member, and no artifact is downloaded.
- Only tags that parse as [semver](https://semver.org) are compared; anything else is excluded rather than guessed at. `1.9.0` is correctly treated as older than `1.10.0` (plain string comparison would get this backwards).
- Pass `--plain-http` or `--ca-file` for a self-hosted registry, matching `tuff pack push`/`pull`.

Anything Tuff cannot check, such as a pack installed without `--reference` or a
local capability with no source at all, reports `not checked` rather than a guessed `up to date`.

### JSON

`tuff outdated --json` prints the rows as an array with the keys `id`, `type`, `target`, `version_scheme`, `current`, `latest`, and `status`.

- Where the table shows `—`, `latest` is `null`, so a script tests for absence rather than for a dash.
- The size of a release change is a separate `change` key; `status` stays `outdated`.
- Every git row carries `latest_release`. On release-pinned rows it is the same value as `latest`.

```sh frame="terminal"
tuff outdated --json | jq '.[] | select(.status == "outdated" or .status == "repointed")'
```

## `tuff policy matrix`

Print, for every agent, how each kind of policy rule is enforced: one row per effect (`deny`, `ask`) and subject (`command`, `read`, `edit`, `mcp`), with its coverage and the mechanism the rule compiles to. It needs no project. `--json` prints one object per agent with its `rules`.

```sh frame="terminal"
tuff policy matrix
```

Claude Code's rows read `partial` for command and file rules and `full` for MCP rules, and every other agent's rows read `unsupported`. `tuff add` of a [policy](/primitives/policies/) prints the caveat for each partially enforced rule and is refused for any selected agent that would not enforce one of its rules.
