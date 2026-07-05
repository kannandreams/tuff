use std::{fs, path::Path, process::Command};

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

fn make_primitive(root: &Path, primitive_id: &str) -> std::path::PathBuf {
    let primitive = root.join("primitive");
    fs::create_dir_all(primitive.join("src")).unwrap();
    fs::write(
        primitive.join("coral.toml"),
        format!(
            r#"id = "{primitive_id}"
version = "1.0.0"
kind = "skill"
description = "Example capability."
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

fn coral() -> Command {
    Command::cargo_bin("coral").unwrap()
}

#[test]
fn version_outputs_current_version() {
    coral()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("coral 0.1.0"));
}

#[test]
fn bare_command_outputs_welcome_menu() {
    coral()
        .assert()
        .success()
        .stdout(predicate::str::contains("Coral"))
        .stdout(predicate::str::contains(
            "is a capability lifecycle manager for coding agents.",
        ))
        .stdout(predicate::str::contains("coral init"))
        .stdout(predicate::str::contains("coral add"))
        .stdout(predicate::str::contains("coral --help"));
}

#[test]
fn cli_lifecycle_reports_clean_modified_and_diff() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "example");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("initialized .coral/lock.json"));
    assert!(temp.path().join(".coral").join("lock.json").exists());

    coral()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--target", "codex"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed example (codex) -> .agents/skills/example/SKILL.md",
        ));

    coral()
        .current_dir(temp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "example\t1.0.0\tcodex\tclean\t.agents/skills/example/SKILL.md",
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

    coral()
        .current_dir(temp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "example\t1.0.0\tcodex\tmodified\t.agents/skills/example/SKILL.md",
        ));

    coral()
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

    coral()
        .current_dir(temp.path())
        .args([
            "add",
            primitive.to_str().unwrap(),
            "--target",
            "codex",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("run 'coral init' first"));
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

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args([
            "add",
            primitive.to_str().unwrap(),
            "--target",
            "codex",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "refusing to overwrite untracked",
        ));
}

#[test]
fn rejects_unknown_target() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "example");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args([
            "add",
            primitive.to_str().unwrap(),
            "--target",
            "nonexistent",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown target"));
}

#[test]
fn target_list_add_remove() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "example");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    // List available adapters
    coral()
        .current_dir(temp.path())
        .args(["target", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("codex"))
        .stdout(predicate::str::contains("claude-code"));

    // Register codex
    coral()
        .current_dir(temp.path())
        .args(["target", "add", "codex"])
        .assert()
        .success()
        .stdout(predicate::str::contains("registered target 'codex'"));

    // Install a skill to codex
    coral()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--target", "codex"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed example (codex) -> .agents/skills/example/SKILL.md",
        ));

    // Remove codex target (should clean up emitted files)
    coral()
        .current_dir(temp.path())
        .args(["target", "remove", "codex"])
        .assert()
        .success()
        .stdout(predicate::str::contains("unregistered target 'codex'"));

    // Emitted file should be cleaned up
    assert!(!temp
        .path()
        .join(".agents")
        .join("skills")
        .join("example")
        .join("SKILL.md")
        .exists());
}

#[test]
fn add_to_multiple_targets() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "example");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args([
            "add",
            primitive.to_str().unwrap(),
            "--target",
            "codex",
            "--target",
            "claude-code",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed example (codex) -> .agents/skills/example/SKILL.md",
        ))
        .stdout(predicate::str::contains(
            "installed example (claude-code) -> .claude/skills/example/SKILL.md",
        ));

    // Both files should exist
    assert!(temp
        .path()
        .join(".agents")
        .join("skills")
        .join("example")
        .join("SKILL.md")
        .exists());
    assert!(temp
        .path()
        .join(".claude")
        .join("skills")
        .join("example")
        .join("SKILL.md")
        .exists());

    // List should show both targets
    coral()
        .current_dir(temp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "example\t1.0.0\tcodex\tclean\t.agents/skills/example/SKILL.md",
        ))
        .stdout(predicate::str::contains(
            "example\t1.0.0\tclaude-code\tclean\t.claude/skills/example/SKILL.md",
        ));

    // Diff with specific target
    coral()
        .current_dir(temp.path())
        .args(["diff", "example", "--target", "codex"])
        .assert()
        .success();

    // Removing codex should keep claude-code files
    coral()
        .current_dir(temp.path())
        .args(["target", "remove", "codex"])
        .assert()
        .success();

    assert!(temp
        .path()
        .join(".claude")
        .join("skills")
        .join("example")
        .join("SKILL.md")
        .exists());
    assert!(!temp
        .path()
        .join(".agents")
        .join("skills")
        .join("example")
        .join("SKILL.md")
        .exists());
}
