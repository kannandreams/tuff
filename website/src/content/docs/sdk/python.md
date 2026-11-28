---
title: Tuff from Python
description: Install, update, and remove capability packs from a Python application with the tuff-sdk package.
---

`tuff-sdk` is the Tuff SDK for Python. A Python application reads a pack, shows the user exactly what installing it would change, and applies that change, with no `tuff` process to start and no JSON to parse. It runs the same Rust code as the `tuff` command, compiled into the package.

```sh
pip install tuff-sdk
```

The package supports Python 3.9 and later on macOS (Apple silicon and Intel) and Linux (x86_64 and arm64). It is imported as `tuff_sdk`. The command-line tool is the separate `tuffcli` package.

## Behavior

- **Results.** Notes and warnings are lists on each result, such as a note that a hook runs code.
- **Files.** Calls write to the workspace and the baseline folder passed to them. Builds and checkouts use temporary folders, removed when the object that owns them is released.
- **Configuration.** Every location is an argument, so the package runs under a service account or in a container.
- **git.** `Pack.checkout` runs the `git` program on the machine, with its settings and credentials. On a Mac without the Xcode Command Line Tools, the first `git` call prompts the user to install them.
- **Threads.** Plans, applies, builds, and checkouts release the GIL while they run.
- **Types.** The package ships type stubs for every class and method.

## Read a pack

A `Pack` is one release of a pack, ready to plan. Build it from a source folder, read a built `.tuffpack` file, or check it out from git:

```python
import tuff_sdk as tuff

pack = tuff.Pack.build("review-pack")
pack = tuff.Pack.read("review-1.0.0.tuffpack")
pack = tuff.Pack.checkout(
    "https://github.com/example/review-pack",
    targets=["claude"],
    ref="v1.0.0",  # a tag, branch, or commit; the newest release tag when left out
)

print(pack.name, pack.version, pack.digest)
document = pack.inspect()  # the same document as `tuff pack inspect --json`
for member in document["members"]:
    print(member["id"], member["type"], "runs code" if member["runsCode"] else "")
```

`Pack.checkout` also reads a Claude Code plugin, or one plugin of a marketplace with `plugin="name"`, as a pack. `targets` names the harnesses to build for, such as `claude`, `codex`, `cursor`, `opencode`, `pi`, and `open-agents`.

## Install, update, and remove a pack

Each change is two calls. A plan method does the work that can fail and returns the change; `apply` writes it. Show the plan to the user and call `apply` after they confirm.

```python
import tuff_sdk as tuff

workspace = tuff.Workspace.open("my-project")  # or Workspace.init to create one
pack = tuff.Pack.build("review-pack")

plan = workspace.plan_install(pack, ["claude"])
for member in plan.members:
    print(member.change, member.type, member.id)
for file in plan.files:
    print(file.change, file.path)  # "create", "modify", or "delete"
for note in plan.notes:
    print("note:", note)
screen = plan.to_dict()  # the same document as the Rust library's JSON

summary = workspace.apply(plan, tuff.Baselines.dir("/srv/baselines"))
print("installed", summary.pack)
```

A plan is applied once. Applying it again raises `PlanUsedError`, and `plan.used` says whether it was. `apply` raises `DriftError` when a file the plan touches changed after the plan was made, and `RefusedError` while another change holds the workspace, from your application or from `tuff` in a terminal; make the plan again in either case.

Updating takes the new release. `plan_update` returns None when that release is the one installed, and raises `DriftError` when a member has local edits, unless `force=True`. Removing takes the pack's name:

```python
import tuff_sdk as tuff

workspace = tuff.Workspace.open("my-project")
new_release = tuff.Pack.checkout("https://github.com/example/review-pack", targets=["claude"])

plan = workspace.plan_update("com.example/review", new_release)
if plan is None:
    print("up to date")
else:
    for member in plan.members:
        print(member.change, member.id, member.version)
    workspace.apply(plan)

workspace.apply(workspace.plan_remove("com.example/review"))
```

`apply` takes where to keep a copy of each installed folder as it was installed, which `tuff diff` uses to show the original text of an edited file:

| `Baselines` | Where the copies go |
|---|---|
| `Baselines.dir(path)` | a folder you choose |
| `Baselines.in_workspace()` | `.tuff-baselines/` in the project, with a `.gitignore` that keeps it out of commits |
| `Baselines.none()` | nowhere, the default; edits are still detected, from the hashes in `tuff.lock` |

## Apply an approval that comes later

A plan lives in memory, so it does not outlive the process that made it. When an approval can come hours later, after a restart, keep the plan's `fingerprint` with the request. When the approval arrives, make the plan again and pass it to `apply_approved`:

```python
import tuff_sdk as tuff

workspace = tuff.Workspace.open("my-project")
pack = tuff.Pack.build("review-pack")

# When the request comes in: show the plan and keep its fingerprint.
plan = workspace.plan_install(pack, ["claude"])
request = {"plan": plan.to_dict(), "fingerprint": plan.fingerprint}

# When the approval comes in, even after a restart:
plan = workspace.plan_install(pack, ["claude"])
result = workspace.apply_approved(plan, request["fingerprint"])
if isinstance(result, tuff.Applied):
    print("installed", result.summary.pack)
elif isinstance(result, tuff.Changed):
    print("changed since it was approved; ask again:", result.plan.to_dict())
```

The fingerprint covers the pack release, the targets, each member, and every file the plan touches: its path, its contents now, and its contents after the change. Two plans with the same fingerprint make the same change. Any other plan comes back as `Changed` with nothing written, for example when the pack's author published a new release or a file the plan touches was edited.

## Errors

Every error is a `TuffError` with `kind`, the same kind as the `kind` field of the CLI's JSON errors, and `hint`, the suggestion to show, or None. Each kind has its own class:

| Class | `kind` | When |
|---|---|---|
| `UsageError` | `usage` | The request is not valid |
| `NotFoundError` | `not_found` | Something it names does not exist |
| `RefusedError` | `refused` | Another change holds the workspace |
| `DriftError` | `drift` | Files changed since the plan was made, or a member has local edits |
| `SourceError` | `source` | A source could not be read, such as a git address |
| `CorruptError` | `corrupt` | A file Tuff reads is damaged |
| `UnsupportedError` | `unsupported` | The pack is not built for a target, or similar |
| `IoError` | `io` | Reading or writing a file failed |
| `PlanUsedError` | `usage` | The plan was already applied |

```python
import tuff_sdk as tuff

try:
    workspace = tuff.Workspace.open("my-project")
except tuff.TuffError as error:
    print(error.kind, error, error.hint)
```

## Stability

The package's API is 0.x. It can change in a minor release, and each change is in the [changelog](/changelog/). It is released with Tuff under the same version number, so a `tuff-sdk` release runs the same code as the `tuff` release with that number. The JSON documents in the [Pack JSON Specification](/spec/packs/) are the stable interface, and `Pack.inspect()` returns exactly that document.
