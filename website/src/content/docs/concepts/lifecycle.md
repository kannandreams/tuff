---
title: Lifecycle & Drift Detection
description: How Coral records baselines, reports drift, and updates tracked capabilities.
---

Coral is built around capability lifecycle management, not just file installation.

The core loop is:

1. install or import a capability into a target
2. record the install-time baseline
3. allow the repo to customize the emitted files
4. detect drift relative to the recorded baseline
5. make that drift visible through listing, diffing, checking, and updates

This is the main reason Coral exists. Teams need project-owned capabilities that can evolve without
losing provenance, source metadata, or update behavior.

## Coral and Git

Git is still the system of record for repository history. Coral adds a different layer of state.

Git can tell you that a file changed. Coral also records:

- which capability produced that file
- which target emitted it
- which baseline version it came from
- whether it was installed from a local source or git source
- which upstream revision it was pinned to

That extra metadata is the part plain Git history does not carry.

It also means teams can relate capability version changes to downstream effects such as:

- agent behavior changes
- review outcomes
- prompt quality shifts
- performance or evaluation metrics tracked outside Coral

Coral does not replace Git. It versions the capability lifecycle metadata around the files Git is
already storing.

## Baseline, local, and upstream

Coral models every tracked emitted file as up to three states:

| Version | Source |
|---|---|
| **Baseline** | Content recorded at last install, import, or update |
| **Local** | Current file in `.agents/` or `.claude/` |
| **Upstream** | Latest file content from the git source, if the capability was installed from git |

## Drift states

When Coral compares local files against the recorded baseline, the common states are:

| State | Meaning |
|---|---|
| `clean` | Installed content matches the recorded baseline |
| `modified` | Local content differs from the recorded baseline |
| `missing` | A tracked emitted file no longer exists |

## Local capability lifecycle

For local capabilities, the typical flow is:

```sh frame="terminal"
coral create --skill my-skill
coral import .agents/skills/my-skill -t open-agents
coral list
coral diff my-skill
```

If the drift is intentional and should become the new baseline:

```sh frame="terminal"
coral import .agents/skills/my-skill -t open-agents --override
```

## Cleanup

Cleanup is explicit about file ownership. For a capability Coral installed by
copying files into a target, delete only the generated target files:

```sh frame="terminal"
coral delete my-skill -t open-agents
```

For a capability imported from an existing `.agents/` or `.claude/` directory,
remove Coral tracking without touching the files:

```sh frame="terminal"
coral untrack my-skill -t open-agents
```

Both commands require a target. `delete` refuses imported capabilities and
requires `--force` for locally modified generated files. `untrack` removes the
target lock entry and baseline while preserving the capability files,
`coral.toml`, and MCP configuration. The original source directory is never
deleted by `delete`.

## Git-sourced capability lifecycle

For git-backed capabilities, Coral can compare baseline, local, and upstream together:

```sh frame="terminal"
coral outdated
coral diff rust-implement --upstream
coral update rust-implement --check
coral update rust-implement
```

## Update behavior

The update path for git-sourced capabilities works like this:

| Local | Upstream | Behavior |
|---|---|---|
| clean | unchanged | No-op |
| clean | changed | Apply upstream, refresh baseline |
| modified | unchanged | Keep local state, report drift |
| modified | changed | Attempt three-way merge |
| conflict | changed | Report conflicts and preserve local files |

If you want to replace the local customized copy with upstream output, use:

```sh frame="terminal"
coral update <id> --force
```

## Commands in the lifecycle

| Command | Purpose |
|---|---|
| `coral list` | Show drift status and target paths |
| `coral diff <id>` | Show local changes against baseline |
| `coral diff <id> --upstream` | Show upstream changes against baseline |
| `coral check` | Fail CI when tracked files drift |
| `coral outdated` | Show whether git-sourced capabilities have newer revisions |
| `coral update <id>` | Reconcile a git-sourced capability with upstream |
| `coral delete <id> -t <target>` | Delete Coral-generated files for a target |
| `coral untrack <id> -t <target>` | Remove tracking while preserving files |

## Capability Metadata

The important difference is not just that Coral detects drift. It keeps the capability metadata
attached to the emitted files for the whole lifecycle:

- source
- version
- target
- baseline
- scope
- update path

That makes the capability observable over time instead of becoming another copied file in the repo.
