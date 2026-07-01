use std::{fs, path::Path, process::Command};

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

fn make_primitive(root: &Path, primitive_id: &str) -> std::path::PathBuf {
    let primitive = root.join("primitive");
    fs::create_dir_all(primitive.join("src")).unwrap();
    fs::write(
        primitive.join("loadout.toml"),
        format!(
            r#"id = "{primitive_id}"
version = "1.0.0"
kind = "skill"
target = "codex"
description = "Example primitive."
files = ["src/SKILL.md"]
"#
        ),
    )
    .unwrap();
    fs::write(
        primitive.join("src").join("SKILL.md"),
        "# Example\n\nOriginal text.\n",
    )
    .unwrap();
    primitive
}

fn loadout() -> Command {
    Command::cargo_bin("loadout").unwrap()
}

#[test]
fn version_outputs_current_version() {
    loadout()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("loadout 0.1.0"));
}

#[test]
fn cli_lifecycle_reports_clean_modified_and_diff() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "example");

    loadout()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("initialized .loadout/lock.json"));
    assert!(temp.path().join(".loadout").join("lock.json").exists());

    loadout()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed example -> .agents/skills/example/SKILL.md",
        ));

    loadout()
        .current_dir(temp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "example\t1.0.0\tclean\t.agents/skills/example/SKILL.md",
        ));

    fs::write(
        temp.path()
            .join(".agents")
            .join("skills")
            .join("example")
            .join("SKILL.md"),
        "# Example\n\nChanged text.\n",
    )
    .unwrap();

    loadout()
        .current_dir(temp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "example\t1.0.0\tmodified\t.agents/skills/example/SKILL.md",
        ));

    loadout()
        .current_dir(temp.path())
        .args(["diff", "example"])
        .assert()
        .success()
        .stdout(predicate::str::contains("-Original text."))
        .stdout(predicate::str::contains("+Changed text."));
}

#[test]
fn add_requires_init() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "example");

    loadout()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("run 'loadout init' first"));
}

#[test]
fn add_refuses_to_overwrite_untracked_skill() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "example");
    let existing_skill = temp
        .path()
        .join(".agents")
        .join("skills")
        .join("example")
        .join("SKILL.md");
    fs::create_dir_all(existing_skill.parent().unwrap()).unwrap();
    fs::write(existing_skill, "# Existing\n").unwrap();

    loadout()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    loadout()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "refusing to overwrite untracked skill",
        ));
}

#[test]
fn rejects_unsupported_target() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "example");
    fs::write(
        primitive.join("loadout.toml"),
        r#"id = "example"
version = "1.0.0"
kind = "skill"
target = "claude-code"
description = "Example primitive."
files = ["src/SKILL.md"]
"#,
    )
    .unwrap();

    loadout()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    loadout()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported primitive target"));
}
