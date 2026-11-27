//! The library used the way an application uses it: no `HOME`, no XDG
//! folders, and every output folder named by the caller.

use std::fs;
use std::path::{Path, PathBuf};

use tuff_core::bundle;
use tuff_packs::build::{build_as, build_for_declared_targets};
use tuff_packs::import::{ImportRequest, import};
use tuff_packs::inspect;
use tuff_packs::marketplace::{MarketplaceBuildOptions, build_marketplace};

fn example_pack() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/packs/example/review-pack")
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn write_plugin(root: &Path) {
    write(
        &root.join(".claude-plugin/plugin.json"),
        r#"{ "name": "review-kit", "version": "1.2.0", "description": "Review helpers." }"#,
    );
    write(
        &root.join("skills/review/SKILL.md"),
        "---\nname: review\ndescription: Review a pull request.\n---\n\nReview it.\n",
    );
    write(&root.join("commands/ship.md"), "Ship it.\n");
}

/// One test, because it changes the process environment.
#[test]
fn packs_build_inspect_import_and_publish_without_home() {
    // SAFETY: this is the only test in this binary, so no other thread
    // reads the environment while it changes.
    unsafe {
        for name in [
            "HOME",
            "XDG_CACHE_HOME",
            "XDG_CONFIG_HOME",
            "XDG_STATE_HOME",
            "XDG_DATA_HOME",
        ] {
            std::env::remove_var(name);
        }
    }

    // Build and inspect a source pack.
    let built = build_for_declared_targets(&example_pack()).unwrap();
    let source = serde_json::json!({ "kind": "folder", "path": "review-pack" });
    let document = inspect::document(&built.artifact, &source).unwrap();
    assert_eq!(document.schema, inspect::INSPECT_SCHEMA);
    assert_eq!(document.name, "com.example/review");
    assert_eq!(document.targets, ["claude", "open-agents"]);
    let hook = document
        .members
        .iter()
        .find(|member| member.id == "lint-hook")
        .expect("the hook member");
    assert!(hook.runs_code);
    let json = serde_json::to_value(&document).unwrap();
    assert_eq!(json["x"]["example-host"]["default_level"], "read");

    // Read a Claude Code plugin as a pack and write it where we choose.
    let work = tempfile::tempdir().unwrap();
    let plugin = work.path().join("plugin");
    write_plugin(&plugin);
    let imported = import(ImportRequest {
        source: plugin.to_str().unwrap(),
        plugin: None,
        git_ref: None,
        path: None,
    })
    .unwrap();
    assert_eq!(imported.manifest.name, "review-kit");
    assert!(
        imported
            .unsupported()
            .iter()
            .any(|item| item.kind == "commands"),
        "{:?}",
        imported.unsupported()
    );
    let pack = work.path().join("pack");
    bundle::write_source_pack(&pack, &imported).unwrap();

    // Write it back out as a plugin, then as a marketplace.
    let as_plugin = build_as(&pack, &["claude-plugin".to_string()], false).unwrap();
    assert!(
        as_plugin
            .artifact
            .contents
            .iter()
            .any(|content| content.path == "targets/claude-plugin/skills/review/SKILL.md")
    );
    let market = work.path().join("market");
    let built = build_marketplace(MarketplaceBuildOptions {
        sources: std::slice::from_ref(&pack),
        output: &market,
        name: Some("acme"),
        owner_name: "Acme",
        owner_url: None,
        skip_unsupported: false,
    })
    .unwrap();
    assert_eq!(
        built.plugins,
        [("review-kit".to_string(), "1.2.0".to_string())]
    );
    assert!(market.join(".claude-plugin/marketplace.json").is_file());
    assert!(
        market
            .join("plugins/review-kit/skills/review/SKILL.md")
            .is_file()
    );
}
