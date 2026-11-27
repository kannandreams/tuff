---
title: Packs from Rust
description: Build, inspect, import, and check packs and Claude Code plugins from a Rust application with the tuff-packs crate.
---

`tuff-packs` is the library the `tuff pack` commands are built on. A Rust application calls it directly, with no `tuff` process to start and no JSON to parse. It covers reading and building packs: inspecting a pack for a review screen, reading a Claude Code plugin as a pack, checking a git pack for a newer release, previewing an update, and writing packs as a Claude Code plugin or marketplace.

Installing, updating, and removing packs are not in the crate yet. Run those as `tuff` commands with `--json`, as [Packs in Your Application](/guides/packs-in-your-application/) describes. An application in another language uses that guide for everything.

:::note[Version]
`tuff-packs` is published from Tuff 0.19.0. Until then, depend on the repository:

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
- **It writes only where you tell it.** Output folders are arguments. Builds and checkouts use temporary folders from the system's temporary directory, and each is deleted when the value that owns it is dropped.
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

## Stability

The crate's Rust API is 0.x. It can change in a minor release, and each change is in the [changelog](/changelog/). The JSON documents in the [Pack JSON Specification](/spec/packs/) are the stable interface, and the library's inspect and preview documents serialize to exactly those. API reference: [docs.rs/tuff-packs](https://docs.rs/tuff-packs).
