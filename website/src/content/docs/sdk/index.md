---
title: Tuff SDK
description: Install, update, and remove agent capabilities from your own application, in Python, Rust, or any language through JSON.
---

The Tuff SDK puts Tuff inside your application. An agent platform, an internal tool, or anything with a "plugins" page can install packs of skills, MCP servers, and hooks into a project, show the user exactly what will change before anything is written, and keep track of what is installed, with the same engine as the `tuff` command.

| | Python | Rust | Any language |
|---|---|---|---|
| Package | [`tuff-sdk`](https://pypi.org/project/tuff-sdk/) on PyPI | [`tuff-packs`](https://crates.io/crates/tuff-packs) on crates.io | the `tuff` command |
| Install | `pip install tuff-sdk` | `cargo add tuff-packs tuff-core` | [Installation](/installation/) |
| Runs | in process | in process | one process per call, with `--json` |
| Guide | [Tuff from Python](/sdk/python/) | [The tuff-packs Rust library](/sdk/rust/) | [Packs in Your Application](/guides/packs-in-your-application/) |

## What every SDK does the same way

- **Plan, then apply.** A plan lists every member and every file that a change will create, modify, or delete, and writes nothing. Apply writes exactly that plan, and refuses if anything it touches changed in between.
- **Approvals that come later.** Each plan has a fingerprint. Keep it with an approval, plan again when the approval arrives, and apply only if the fingerprint matches, even after a restart.
- **One change at a time.** Apply takes a lock on the project, so two changes from your application, or one from your application and one from `tuff` in a terminal, never interleave.
- **It never prints.** Notes and warnings, such as a hook that runs code, are part of the result for you to show.
- **No hidden state.** The project folder and where to keep baseline copies are arguments. Nothing reads `HOME` or the current directory.
- **The same files as the CLI.** An install from the SDK writes the same harness files and `tuff.lock` entries as `tuff pack install`, so `tuff list`, `tuff diff`, and `tuff check` work on the result.

## An install, in Python and Rust

```python
import tuff_sdk as tuff

workspace = tuff.Workspace.open("my-project")
pack = tuff.Pack.build("review-pack")

plan = workspace.plan_install(pack, ["claude"])
for file in plan.files:
    print(file.change, file.path)  # nothing is written yet

workspace.apply(plan)
```

The same install from Rust is in [The tuff-packs Rust library](/sdk/rust/#install-update-and-remove-a-pack).

## What the SDK covers today

Packs: build, inspect, read Claude Code plugins as packs, check out from git, check for newer releases (Rust), preview updates (Rust), write marketplaces (Rust), and install, update, and remove with plan and apply. Single capabilities, and the state of a workspace (what is installed, what was edited, check results) as values, are planned next; until then they are available through the `tuff` command with `--json`.

## Stability

Both libraries are 0.x and can change in a minor release; each change is in the [changelog](/changelog/). They are released with Tuff under the same version number. The JSON documents in the [Pack JSON Specification](/spec/packs/) are the stable interface, and the SDK's inspect and preview documents are exactly those documents.
