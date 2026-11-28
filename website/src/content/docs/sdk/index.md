---
title: Tuff SDK
description: Install, update, and remove agent capabilities from your own application, in Python, Rust, or any language through JSON.
---

The Tuff SDK installs packs of skills, MCP servers, and hooks into a project from your own application. It runs the engine the `tuff` command uses, and returns its results as values for your application to show.

| | Python | Rust | Any language |
|---|---|---|---|
| Package | [`tuff-sdk`](https://pypi.org/project/tuff-sdk/) on PyPI | [`tuff-packs`](https://crates.io/crates/tuff-packs) on crates.io | the `tuff` command |
| Install | `pip install tuff-sdk` | `cargo add tuff-packs tuff-core` | [Installation](/installation/) |
| Runs | in process | in process | one process per call, with `--json` |
| Guide | [Tuff from Python](/sdk/python/) | [The tuff-packs Rust library](/sdk/rust/) | [Packs in Your Application](/guides/packs-in-your-application/) |

## How a change is made

A change takes two calls. The plan call reads the pack and the project and returns each member and file the change creates, modifies, or deletes. The apply call writes those files. Before it writes, it compares each file the plan touches, and `tuff.lock`, with its content when the plan was made, and raises a drift error if any of them differ.

Each plan has a fingerprint, a hash of the change. For an approval that arrives later, keep the fingerprint with the approval request. When the approval arrives, make the plan again and pass it with the fingerprint to `apply_approved`, which writes the plan when the fingerprints match and returns the new plan when they differ.

Apply holds a lock on the project while it writes. A second apply on the same project, from the application or from `tuff` in a terminal, raises a refused error until the first one finishes.

Notes and warnings, such as a note that a hook runs code, are lists on each result. The project folder and the baseline folder are arguments to each call. An install writes the same harness files and `tuff.lock` entries as `tuff pack install`.

## Example

```python
import tuff_sdk as tuff

workspace = tuff.Workspace.open("my-project")
pack = tuff.Pack.build("review-pack")

plan = workspace.plan_install(pack, ["claude"])
for file in plan.files:
    print(file.change, file.path)  # nothing is written yet

workspace.apply(plan)
```

## Coverage

The SDK covers packs: building, inspecting, reading Claude Code plugins as packs, checking out from git, and installing, updating, and removing with plan and apply. The Rust library also checks a git pack for newer releases, previews updates, and writes marketplaces. Single capabilities and the state of a workspace are available through the `tuff` command with `--json`, and are planned for the SDK.

## Stability

Both libraries are 0.x and can change in a minor release; the [changelog](/changelog/) lists each change. They are released with Tuff under the same version number. The stable interface is the JSON documents in the [Pack JSON Specification](/spec/packs/), and the SDK's inspect and preview results serialize to those documents.
