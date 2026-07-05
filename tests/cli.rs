use std::{fs, path::Path, process::Command};

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

fn make_git_skill_repo(root: &Path) -> std::path::PathBuf {
    let repo = root.join("skill-repo");
    fs::create_dir_all(repo.join("skills").join("test-skill")).unwrap();
    fs::write(
        repo.join("skills").join("test-skill").join("SKILL.md"),
        "# Test Skill\n\nHello from git-installed skill.\n",
    )
    .unwrap();

    let status = Command::new("git")
        .args(["init"])
        .current_dir(&repo)
        .status()
        .unwrap();
    assert!(status.success(), "git init failed");

    let status = Command::new("git")
        .args(["config", "user.email", "test@coral.test"])
        .current_dir(&repo)
        .status()
        .unwrap();
    assert!(status.success());

    let status = Command::new("git")
        .args(["config", "user.name", "Coral Test"])
        .current_dir(&repo)
        .status()
        .unwrap();
    assert!(status.success());

    let status = Command::new("git")
        .args(["add", "-A"])
        .current_dir(&repo)
        .status()
        .unwrap();
    assert!(status.success());

    let status = Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(&repo)
        .status()
        .unwrap();
    assert!(status.success());

    repo
}

fn make_primitive(root: &Path, primitive_id: &str) -> std::path::PathBuf {
    let primitive = root.join("primitive");
    fs::create_dir_all(primitive.join("src")).unwrap();
    fs::write(
        primitive.join("coral.toml"),
        format!(
            r#"id = "{primitive_id}"
version = "1.0.0"
primitive = "skill"
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
        .stdout(predicate::str::contains("initialized .coral/coral-lock.json"));
    assert!(temp.path().join(".coral").join("coral-lock.json").exists());

    coral()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--target", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed example (open-agents) -> .agents/skills/example/SKILL.md",
        ));

    coral()
        .current_dir(temp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "example\t1.0.0\topen-agents\tclean\t.agents/skills/example/SKILL.md",
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
            "example\t1.0.0\topen-agents\tmodified\t.agents/skills/example/SKILL.md",
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
            "open-agents",
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
            "open-agents",
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
        .stdout(predicate::str::contains("open-agents"))
        .stdout(predicate::str::contains("claude"));

    // Register open-agents
    coral()
        .current_dir(temp.path())
        .args(["target", "add", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains("registered target 'open-agents'"));

    // Install a skill to open-agents
    coral()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--target", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed example (open-agents) -> .agents/skills/example/SKILL.md",
        ));

    // Remove open-agents target (should clean up emitted files)
    coral()
        .current_dir(temp.path())
        .args(["target", "remove", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains("unregistered target 'open-agents'"));

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
            "open-agents",
            "--target",
            "claude",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed example (open-agents) -> .agents/skills/example/SKILL.md",
        ))
        .stdout(predicate::str::contains(
            "installed example (claude) -> .claude/skills/example/SKILL.md",
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
            "example\t1.0.0\topen-agents\tclean\t.agents/skills/example/SKILL.md",
        ))
        .stdout(predicate::str::contains(
            "example\t1.0.0\tclaude\tclean\t.claude/skills/example/SKILL.md",
        ));

    // Diff with specific target
    coral()
        .current_dir(temp.path())
        .args(["diff", "example", "--target", "open-agents"])
        .assert()
        .success();

    // Removing open-agents should keep claude files
    coral()
        .current_dir(temp.path())
        .args(["target", "remove", "open-agents"])
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

#[test]
fn add_git_skill_installs_and_tracks_lifecycle() {
    let temp = TempDir::new().unwrap();
    let repo = make_git_skill_repo(temp.path());
    let repo_url = format!("file://{}", repo.display());

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args([
            "add",
            &repo_url,
            "--target",
            "open-agents",
            "--skill",
            "test-skill",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed test-skill (open-agents) -> .agents/skills/test-skill/SKILL.md",
        ));

    assert!(temp
        .path()
        .join(".agents")
        .join("skills")
        .join("test-skill")
        .join("SKILL.md")
        .exists());

    coral()
        .current_dir(temp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "test-skill\t",
        ))
        .stdout(predicate::str::contains("\tclean\t"));

    fs::write(
        temp.path()
            .join(".agents")
            .join("skills")
            .join("test-skill")
            .join("SKILL.md"),
        "# Modified\n\nChanged from git source.\n",
    )
    .unwrap();

    coral()
        .current_dir(temp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "test-skill\t",
        ))
        .stdout(predicate::str::contains("\tmodified\t"));

    coral()
        .current_dir(temp.path())
        .args(["diff", "test-skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains("-Hello from git-installed skill."))
        .stdout(predicate::str::contains("+Changed from git source."));
}

#[test]
fn add_git_requires_skill_flag() {
    let temp = TempDir::new().unwrap();
    let repo = make_git_skill_repo(temp.path());
    let repo_url = format!("file://{}", repo.display());

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["add", &repo_url, "--target", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--skill is required"));
}

#[test]
fn add_git_missing_skill_reports_error() {
    let temp = TempDir::new().unwrap();
    let repo = make_git_skill_repo(temp.path());
    let repo_url = format!("file://{}", repo.display());

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args([
            "add",
            &repo_url,
            "--target",
            "open-agents",
            "--skill",
            "nonexistent",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn add_git_skill_multi_target() {
    let temp = TempDir::new().unwrap();
    let repo = make_git_skill_repo(temp.path());
    let repo_url = format!("file://{}", repo.display());

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args([
            "add",
            &repo_url,
            "--target",
            "open-agents",
            "--target",
            "claude",
            "--skill",
            "test-skill",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed test-skill (open-agents) -> .agents/skills/test-skill/SKILL.md",
        ))
        .stdout(predicate::str::contains(
            "installed test-skill (claude) -> .claude/skills/test-skill/SKILL.md",
        ));

    assert!(temp
        .path()
        .join(".agents")
        .join("skills")
        .join("test-skill")
        .join("SKILL.md")
        .exists());
    assert!(temp
        .path()
        .join(".claude")
        .join("skills")
        .join("test-skill")
        .join("SKILL.md")
        .exists());
}

#[test]
fn add_git_subfolder_skill() {
    let temp = TempDir::new().unwrap();
    let repo = temp.path().join("skill-repo");

    fs::create_dir_all(
        repo.join("skills")
            .join("security")
            .join("security-review"),
    )
    .unwrap();
    fs::write(
        repo.join("skills")
            .join("security")
            .join("security-review")
            .join("SKILL.md"),
        "# Security Review\n\nCheck for vulns.\n",
    )
    .unwrap();

    fs::create_dir_all(repo.join("skills").join("simple-skill")).unwrap();
    fs::write(
        repo.join("skills").join("simple-skill").join("SKILL.md"),
        "# Simple\n",
    )
    .unwrap();

    let status = Command::new("git")
        .args(["init"])
        .current_dir(&repo)
        .status()
        .unwrap();
    assert!(status.success());

    let _ = Command::new("git")
        .args(["config", "user.email", "test@coral.test"])
        .current_dir(&repo)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.name", "Coral Test"])
        .current_dir(&repo)
        .status();
    let _ = Command::new("git")
        .args(["add", "-A"])
        .current_dir(&repo)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&repo)
        .status();

    let repo_url = format!("file://{}", repo.display());

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args([
            "add",
            &repo_url,
            "--target",
            "open-agents",
            "--skill",
            "security/security-review",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed security/security-review (open-agents) -> .agents/skills/security/security-review/SKILL.md",
        ));

    assert!(temp
        .path()
        .join(".agents")
        .join("skills")
        .join("security")
        .join("security-review")
        .join("SKILL.md")
        .exists());
}

#[test]
fn legacy_alias_codex_works() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "example");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--target", "codex"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed example (open-agents) -> .agents/skills/example/SKILL.md",
        ));

    assert!(temp
        .path()
        .join(".agents")
        .join("skills")
        .join("example")
        .join("SKILL.md")
        .exists());
}

#[test]
fn legacy_alias_claude_code_works() {
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
            "claude-code",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed example (claude) -> .claude/skills/example/SKILL.md",
        ));

    assert!(temp
        .path()
        .join(".claude")
        .join("skills")
        .join("example")
        .join("SKILL.md")
        .exists());
}
