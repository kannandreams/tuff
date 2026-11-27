---
title: Packs from Rust
description: Build, inspect, import, and check packs and Claude Code plugins from a Rust application with the tuff-packs crate.
---

`tuff-packs` is the library the `tuff pack` commands are built on. A Rust application calls it directly, with no `tuff` process to start and no JSON to parse. It inspects a pack for a review screen, reads a Claude Code plugin as a pack, checks a git pack for a newer release, previews an update, writes packs as a Claude Code plugin or marketplace, and installs, updates, and removes packs in a project.

An application in another language runs the `tuff` commands with `--json`, as [Packs in Your Application](/guides/packs-in-your-application/) describes.

:::note[Version]
`tuff-packs` is published from Tuff 0.19.0, and installing, updating, and removing packs need 0.20.0. Until a release has what you need, depend on the repository:

```toml title="Cargo.toml"
[dependencies]
tuff-packs = { git = "https://github.com/kannandreams/tuff" }
tuff-core = { git = "https://github.com/kannandreams/tuff" }
serde_json = "1"
```
:::

```toml title="Cargo.toml"
[dependencies]
tuff-packs = "0.19"
tuff-core = "0.19"
serde_json = "1"
```

`tuff-core` holds the shared types, such as the lockfile and the pack artifact. Keep both crates on the same version.

## What the library does and does not do

- **It never prints.** Each result carries its notes and warnings, such as a harness a plugin's pack is not built for. Show them in your own interface or logs.
- **It writes only where you tell it.** The project and output folders are arguments. Builds and checkouts use temporary folders from the system's temporary directory, and each is deleted when the value that owns it is dropped.
- **It reads no environment variables of its own.** It needs no `HOME`, so it runs under a service account or in a container.
- **git is a program it runs.** Checkouts use the `git` on the machine, with the machine's git settings and credentials, so private repositories work as they do for `git clone`.
- **Errors are `tuff_core::error::TuffError`.** `kind()` is the same kind as the `kind` field of the CLI's JSON errors, and `hint()` is the suggestion to show.

## Inspect a pack for a review screen

Build a source pack folder and describe it. The document is the one `tuff pack inspect --json` prints, so the [Pack JSON Specification](/spec/packs/) describes every field.

```rust
use std::path::Path;

use tuff_packs::build::build_for_declared_targets;
use tuff_packs::inspect;

fn review(folder: &Path) -> tuff_packs::Result<()> {
    let built = build_for_declared_targets(folder)?;
    let source = serde_json::json!({ "kind": "folder", "path": folder.display().to_string() });
    let document = inspect::document(&built.artifact, &source)?;

    for member in &document.members {
        println!(
            "{} ({:?}) runs code: {}",
            member.id, member.capability_type, member.runs_code
        );
        if let Some(mcp) = &member.mcp {
            println!("  needs {:?}", mcp.env);
        }
    }
    for item in document.unsupported {
        println!("not carried: {} ({})", item.kind, item.reason);
    }
    // The same document as JSON, for a web front end.
    println!("{}", serde_json::to_string(&document)?);
    Ok(())
}
```

`built` owns the temporary folder holding the artifact. Keep it alive while you read `built.artifact`.

## Inspect a pack or plugin in a git repository

Check the repository out at its newest release tag, or at a tag, branch, or commit you name. A repository may hold a `tuff-pack.toml` or a Claude Code plugin or marketplace; `source_pack_of` imports a plugin into a temporary folder.

```rust
use tuff_packs::build::build_for_declared_targets;
use tuff_packs::git::{CheckoutRequest, checkout};
use tuff_packs::import::source_pack_of;
use tuff_packs::inspect;

fn review_git(url: &str) -> tuff_packs::Result<()> {
    let checked_out = checkout(CheckoutRequest {
        url,
        path: None,          // the folder inside the repository
        git_ref: None,       // newest release tag
        plugin: None,        // a marketplace entry
        allow_remote: false, // keep to this repository's tags
    })?;
    let pack = source_pack_of(&checked_out)?;
    let built = build_for_declared_targets(&pack.root)?;

    let mut source = serde_json::to_value(&checked_out.source)?;
    source["kind"] = "git".into();
    let document = inspect::document(&built.artifact, &source)?;

    // The commit to install, so what is installed is what was reviewed.
    println!("{} at {}", document.version, checked_out.source.git_ref);
    for note in pack.notes.iter().chain(&built.notes) {
        println!("note: {note}");
    }
    Ok(())
}
```

Install the reviewed release with `tuff add pack <url> --ref <tag> --json` and compare its `source.ref` with `checked_out.source.git_ref`.

## Check an installed git pack for a newer release

`git::status` compares an installed pack with its repository, using the source recorded in `tuff.lock`:

```rust
use std::path::Path;

use tuff_core::lockfile::{self, PackSource};
use tuff_packs::git::{GitPackStatus, status};

fn check(project: &Path) -> tuff_packs::Result<()> {
    let lock = lockfile::require_lockfile(project)?;
    for (name, pack) in lock.normalized_packs() {
        let PackSource::Git(git) = &pack.source else {
            continue;
        };
        match status(&name, &pack.version, git)?.0 {
            GitPackStatus::Newer { version, .. } => println!("{name}: {version} is available"),
            GitPackStatus::Repointed { .. } => {
                println!("{name}: its tag moved; review before updating")
            }
            GitPackStatus::TagMissing => println!("{name}: its tag was deleted"),
            GitPackStatus::BranchMoved { commit } => println!("{name}: branch moved to {commit}"),
            GitPackStatus::Current | GitPackStatus::Pinned => {}
        }
    }
    Ok(())
}
```

`tuff_packs::preview::preview_update` builds the same preview as `tuff update <member> --check --json` from an installed pack and a built release, and `tuff_packs::preview::dirty_pack_members` lists the members a user has edited.

## Read a Claude Code plugin as a pack

```rust
use std::path::Path;

use tuff_core::bundle::write_source_pack;
use tuff_packs::import::{ImportRequest, import};

fn convert(plugin: &str, output: &Path) -> tuff_packs::Result<()> {
    let imported = import(ImportRequest {
        source: plugin, // a folder or a git URL
        plugin: None,   // a marketplace entry
        git_ref: None,
        path: None,
    })?;
    for item in imported.unsupported() {
        println!("not carried: {} {}", item.kind, item.reason);
    }
    write_source_pack(output, &imported)
}
```

This is `tuff pack import`: skills and MCP servers become members, and the rest is listed and kept under `extra/`.

## Write packs as a Claude Code plugin or marketplace

```rust
use std::path::{Path, PathBuf};

use tuff_packs::build::build_as;
use tuff_packs::marketplace::{MarketplaceBuildOptions, build_marketplace};

fn publish(packs: &[PathBuf], output: &Path) -> tuff_packs::Result<()> {
    // One pack as a plugin, held in the artifact under targets/claude-plugin/.
    let plugin = build_as(&packs[0], &["claude-plugin".to_string()], false)?;
    println!("{} files", plugin.artifact.contents.len());

    // Every pack as one marketplace folder.
    let built = build_marketplace(MarketplaceBuildOptions {
        sources: packs,
        output,
        name: Some("acme"),
        owner_name: "Acme",
        owner_url: Some("https://example.com"),
        skip_unsupported: false,
    })?;
    for (name, version) in &built.plugins {
        println!("{name} {version}");
    }
    Ok(())
}
```

`skip_unsupported: true` leaves out members a plugin cannot hold and reports each in `notes`; with `false` the build fails and names them.

## Install, update, and remove a pack

Each change to a project is two calls. A plan function does everything that can fail and writes nothing; `apply` writes the plan. Show the plan to the user, and apply it once they confirm, so what is installed is what they saw.

```rust
use std::path::Path;

use tuff_core::lockfile::PackSource;
use tuff_packs::build::build_for_targets;
use tuff_packs::git::{CheckoutRequest, checkout};
use tuff_packs::import::source_pack_of;
use tuff_packs::install::{apply, plan_install};
use tuff_packs::workspace::{Baselines, Workspace};

fn install(project: &Path, url: &str, baselines: &Path) -> tuff_packs::Result<()> {
    let workspace = Workspace::open(project)?;
    let checked_out = checkout(CheckoutRequest {
        url,
        path: None,
        git_ref: None,
        plugin: None,
        allow_remote: false,
    })?;
    let pack = source_pack_of(&checked_out)?;
    let targets = ["claude".to_string()];
    let built = build_for_targets(&pack.root, &targets)?;
    let source = PackSource::Git(checked_out.source.clone());

    let plan = plan_install(&workspace, &built.artifact, &source, &targets)?;
    for file in &plan.files {
        println!("{:?} {}", file.change, file.path);
    }
    for note in &plan.notes {
        println!("note: {note}");
    }
    // The same plan as JSON, for a confirmation screen.
    println!("{}", serde_json::to_string(&plan)?);

    apply(&workspace, &Baselines::dir(baselines), plan)?;
    Ok(())
}
```

`Workspace::open` takes a project folder that holds a `tuff.lock`, and `Workspace::init` creates the folder and an empty lockfile. The plan lists each member it adds, updates, or removes, every file it creates, modifies, or deletes (`tuff.lock` included), and notes such as a hook that runs code.

`apply` refuses with a `drift` error when a file the plan touches changed after the plan was made; make the plan again. It takes a lock on the project first, so a second change to the same project, from your application or from `tuff` in a terminal, fails with a `refused` error until the first is done. The lock is a file in the system's temporary directory, and nothing is added to the project.

The second argument to `apply` says where to keep a copy of each installed folder as it was installed, which `tuff diff` uses to show the original text of an edited file:

| `Baselines` | Where the copies go |
|---|---|
| `Baselines::dir(path)` | a folder you choose |
| `Baselines::in_workspace()` | `.tuff-baselines/` in the project, with a `.gitignore` that keeps it out of commits |
| `Baselines::none()` | nowhere; edits are still detected, from the hashes in `tuff.lock` |
| `Baselines::custom(store)` | your own storage, through the `BaselineStore` trait |

Updating takes the new release, built or read the same way. `plan_update` returns `None` when that release is the one installed, and refuses when a member has local edits unless `force` is set:

```rust
use std::path::Path;

use tuff_core::lockfile::PackSource;
use tuff_core::pack::PackArtifact;
use tuff_packs::install::{UpdateRequest, apply, plan_remove, plan_update};
use tuff_packs::workspace::{Baselines, Workspace};

fn update(
    project: &Path,
    pack: &str,
    artifact: &PackArtifact,
    source: &PackSource,
) -> tuff_packs::Result<()> {
    let workspace = Workspace::open(project)?;
    let request = UpdateRequest {
        pack,
        artifact,
        source,
        targets: &[], // every harness the pack is installed for
        force: false,
    };
    match plan_update(&workspace, request)? {
        None => println!("{pack} is up to date"),
        Some(plan) => {
            for member in &plan.members {
                println!("{:?} {}", member.change, member.id);
            }
            apply(&workspace, &Baselines::none(), plan)?;
        }
    }
    Ok(())
}

fn remove(project: &Path, pack: &str) -> tuff_packs::Result<()> {
    let workspace = Workspace::open(project)?;
    let plan = plan_remove(&workspace, pack, false)?;
    apply(&workspace, &Baselines::none(), plan)?;
    Ok(())
}
```

A pack's members are installed into the project. The `tuff` command's global scope, under the home directory, is not part of the library.

## Stability

The crate's Rust API is 0.x. It can change in a minor release, and each change is in the [changelog](/changelog/). The JSON documents in the [Pack JSON Specification](/spec/packs/) are the stable interface, and the library's inspect and preview documents serialize to exactly those. A plan's JSON is not in the specification yet; read it through the Rust types. API reference: [docs.rs/tuff-packs](https://docs.rs/tuff-packs).
