//! The examples on "The tuff-packs Rust library" guide page
//! (website/src/content/docs/sdk/rust.md), compiled so the
//! page stays in step with the API. tests/guide_examples.rs checks that
//! every block on the page is here.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use tuff_core::bundle::write_source_pack;
use tuff_core::lockfile::{self, PackArtifactSource, PackSource};
use tuff_core::pack::PackArtifact;
use tuff_packs::build::build_as;
use tuff_packs::build::build_for_declared_targets;
use tuff_packs::build::build_for_targets;
use tuff_packs::git::{CheckoutRequest, checkout};
use tuff_packs::git::{GitPackStatus, status};
use tuff_packs::import::source_pack_of;
use tuff_packs::import::{ImportRequest, import};
use tuff_packs::inspect;
use tuff_packs::install::{Approval, apply_approved};
use tuff_packs::install::{UpdateRequest, apply, plan_install, plan_remove, plan_update};
use tuff_packs::marketplace::{MarketplaceBuildOptions, build_marketplace};
use tuff_packs::workspace::{Baselines, Workspace};

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

/// When the request comes in: show the plan and keep its fingerprint.
fn ask(project: &Path, artifact: &PackArtifact, source: &PackSource) -> tuff_packs::Result<String> {
    let workspace = Workspace::open(project)?;
    let plan = plan_install(&workspace, artifact, source, &["claude".to_string()])?;
    println!("{}", serde_json::to_string(&plan)?);
    Ok(plan.fingerprint.clone())
}

/// When the approval comes in, even after a restart: make the plan again
/// and apply it only if it is the change that was approved.
fn approve(
    project: &Path,
    artifact: &PackArtifact,
    source: &PackSource,
    approved: &str,
) -> tuff_packs::Result<()> {
    let workspace = Workspace::open(project)?;
    let plan = plan_install(&workspace, artifact, source, &["claude".to_string()])?;
    match apply_approved(&workspace, &Baselines::none(), plan, approved)? {
        Approval::Applied(summary) => println!("installed {}", summary.pack),
        // Something changed since: show the new plan and ask again.
        Approval::Changed(plan) => println!("{}", serde_json::to_string(&plan)?),
    }
    Ok(())
}

/// The landing page's Rust example (website/src/data/sdk/install.rs).
fn landing() -> tuff_packs::Result<()> {
    // 1. Open your project and read a pack.
    let workspace = Workspace::open("my-project")?;
    let pack = build_for_declared_targets("review-pack".as_ref())?;
    let source = PackSource::Artifact(PackArtifactSource { registry: None });

    // 2. Plan the install. Nothing is written yet.
    let plan = plan_install(&workspace, &pack.artifact, &source, &["claude".into()])?;
    for file in &plan.files {
        println!("{:?} {}", file.change, file.path);
    }

    // 3. Apply the plan once the user approves it.
    apply(&workspace, &Baselines::none(), plan)?;
    Ok(())
}

fn main() {}
