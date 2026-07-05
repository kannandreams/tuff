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
type = "skill"
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

fn make_tool_primitive(root: &Path, tool_id: &str) -> std::path::PathBuf {
    let primitive = root.join("tool-primitive");
    fs::create_dir_all(&primitive).unwrap();
    fs::write(
        primitive.join("coral.toml"),
        format!(
            r#"id = "{tool_id}"
version = "1.0.0"
type = "tool"
description = "A test tool."
files = ["run.sh"]

[parameters]
type = "object"
required = ["target"]

[parameters.properties.target]
type = "string"
description = "The target to scan"

[implementation]
language = "bash"
entrypoint = "run.sh"
runtime_deps = ["curl"]
"#
        ),
    )
    .unwrap();
    fs::write(primitive.join("run.sh"), "#!/bin/bash\necho \"scanning: $1\"\n").unwrap();
    primitive
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
            "example\t1.0.0\tproject\topen-agents\tclean\t.agents/skills/example/SKILL.md",
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
            "example\t1.0.0\tproject\topen-agents\tmodified\t.agents/skills/example/SKILL.md",
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
            "example\t1.0.0\tproject\topen-agents\tclean\t.agents/skills/example/SKILL.md",
        ))
        .stdout(predicate::str::contains(
            "example\t1.0.0\tproject\tclaude\tclean\t.claude/skills/example/SKILL.md",
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
        .stderr(predicate::str::contains("--skill, --tool, or --hook is required"));
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

#[test]
fn add_global_creates_lockfile_and_emits_to_home() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "global-skill");
    let home_env = std::env::var("HOME").unwrap();
    let home = std::path::Path::new(&home_env);

    // Cleanup from previous runs
    let _ = std::fs::remove_file(home.join(".coral").join("coral-lock.json"));
    let _ = std::fs::remove_dir_all(home.join(".agents").join("skills").join("global-skill"));
    let _ = std::fs::remove_dir_all(home.join(".coral").join("baselines"));

    coral()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--target", "open-agents", "--global"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed global-skill (open-agents)",
        ));

    assert!(home.join(".coral").join("coral-lock.json").exists());

    // Cleanup
    let _ = std::fs::remove_file(home.join(".coral").join("coral-lock.json"));
    let _ = std::fs::remove_dir_all(home.join(".agents").join("skills").join("global-skill"));
    let _ = std::fs::remove_dir_all(home.join(".coral").join("baselines"));
}

#[test]
fn list_shows_scope_column() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "example");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--target", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("project"))
        .stdout(predicate::str::contains("example\t1.0.0\tproject\topen-agents\tclean\t"));
}

#[test]
fn remove_primitive_cleans_up() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "remove-test");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--target", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["remove", "remove-test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed 'remove-test' from project scope"));

    assert!(!temp
        .path()
        .join(".agents")
        .join("skills")
        .join("remove-test")
        .join("SKILL.md")
        .exists());

    // Only check project scope is empty
    coral()
        .current_dir(temp.path())
        .args(["list", "--scope", "project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no capabilities installed"));
}

#[test]
fn status_shows_override_warning() {
    let temp = TempDir::new().unwrap();
    let primitive_a = make_primitive(temp.path(), "dup-override");
    let home_env = std::env::var("HOME").unwrap();
    let home = std::path::Path::new(&home_env);

    // Cleanup from previous runs
    let _ = std::fs::remove_file(home.join(".coral").join("coral-lock.json"));
    let _ = std::fs::remove_dir_all(home.join(".agents").join("skills").join("dup-override"));
    let _ = std::fs::remove_dir_all(home.join(".coral").join("baselines").join("open-agents").join("dup-override"));

    coral()
        .current_dir(temp.path())
        .args(["add", primitive_a.to_str().unwrap(), "--target", "open-agents", "--global"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", primitive_a.to_str().unwrap(), "--target", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("dup-override"))
        .stdout(predicate::str::contains("[overrides global"))
        .stdout(predicate::str::contains("[shadowed by project copy]"));

    // Cleanup
    let _ = std::fs::remove_file(home.join(".coral").join("coral-lock.json"));
    let _ = std::fs::remove_dir_all(home.join(".agents").join("skills").join("dup-override"));
    let _ = std::fs::remove_dir_all(home.join(".coral").join("baselines"));
}

#[test]
fn update_git_skill_reports_up_to_date() {
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
        .success();

    coral()
        .current_dir(temp.path())
        .args(["update", "test-skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already up to date"));
}

#[test]
fn add_tool_installs_and_emits() {
    let temp = TempDir::new().unwrap();
    let tool = make_tool_primitive(temp.path(), "scan-tool");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["add", tool.to_str().unwrap(), "--target", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed scan-tool (open-agents) -> .agents/tools/scan-tool/run.sh",
        ));

    assert!(temp
        .path()
        .join(".agents")
        .join("tools")
        .join("scan-tool")
        .join("run.sh")
        .exists());
}

#[test]
fn add_tool_rejects_invalid_schema() {
    let temp = TempDir::new().unwrap();
    let primitive = temp.path().join("bad-tool");
    fs::create_dir_all(&primitive).unwrap();
    fs::write(
        primitive.join("coral.toml"),
        r#"id = "bad-tool"
version = "1.0.0"
type = "tool"
description = "Bad tool."

[parameters]
foo = "bar"

[implementation]
language = "bash"
entrypoint = "run.sh"
"#,
    )
    .unwrap();
    fs::write(primitive.join("run.sh"), "echo test\n").unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--target", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("parameters 'type' must be 'object'"));
}

#[test]
fn add_tool_rejects_path_traversal() {
    let temp = TempDir::new().unwrap();
    let primitive = temp.path().join("bad-tool");
    fs::create_dir_all(&primitive).unwrap();
    fs::write(
        primitive.join("coral.toml"),
        r#"id = "bad-tool"
version = "1.0.0"
type = "tool"
description = "Bad tool."

[parameters]
type = "object"
required = ["x"]

[parameters.properties.x]
type = "string"
description = "x"

[implementation]
language = "bash"
entrypoint = "../etc/passwd"
"#,
    )
    .unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--target", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("path traversal"));
}

#[test]
fn add_tool_shows_runtime_deps() {
    let temp = TempDir::new().unwrap();
    let tool = make_tool_primitive(temp.path(), "scan-tool");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["add", tool.to_str().unwrap(), "--target", "open-agents"])
        .assert()
        .success()
        .stderr(predicate::str::contains("this tool requires runtime dependencies: curl"));
}

#[test]
fn list_filter_by_primitive_kind() {
    let temp = TempDir::new().unwrap();
    let skill = make_primitive(temp.path(), "my-skill");
    let tool = make_tool_primitive(temp.path(), "my-tool");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "--target", "open-agents"])
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", tool.to_str().unwrap(), "--target", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["list", "--type", "tool"])
        .assert()
        .success()
        .stdout(predicate::str::contains("my-tool"))
        .stdout(predicate::str::contains("my-skill").not());

    coral()
        .current_dir(temp.path())
        .args(["list", "--type", "skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains("my-skill"))
        .stdout(predicate::str::contains("my-tool").not());
}

#[test]
fn add_tool_multi_target_with_mcp() {
    let temp = TempDir::new().unwrap();
    let tool = make_tool_primitive(temp.path(), "scan-tool");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args([
            "add",
            tool.to_str().unwrap(),
            "--target",
            "open-agents",
            "--target",
            "claude",
        ])
        .assert()
        .success();

    assert!(temp
        .path()
        .join(".agents")
        .join("tools")
        .join("scan-tool")
        .join("run.sh")
        .exists());
    assert!(temp
        .path()
        .join(".claude")
        .join("tools")
        .join("scan-tool")
        .join("run.sh")
        .exists());

    // Both MCP configs should exist
    let agents_mcp = temp.path().join(".agents").join("mcp.json");
    let claude_mcp = temp.path().join(".mcp.json");
    assert!(agents_mcp.exists());
    assert!(claude_mcp.exists());
}

#[test]
fn remove_tool_cleans_mcp_entry() {
    let temp = TempDir::new().unwrap();
    let tool = make_tool_primitive(temp.path(), "scan-tool");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", tool.to_str().unwrap(), "--target", "claude"])
        .assert()
        .success();

    let mcp_path = temp.path().join(".mcp.json");
    assert!(mcp_path.exists());
    let before: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&mcp_path).unwrap()).unwrap();
    assert!(before["mcpServers"]["scan-tool"].is_object());

    coral()
        .current_dir(temp.path())
        .args(["remove", "scan-tool"])
        .assert()
        .success();

    let after: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&mcp_path).unwrap()).unwrap();
    assert!(after["mcpServers"]["scan-tool"].is_null());
}

fn make_hook_primitive(root: &Path, hook_id: &str) -> std::path::PathBuf {
    let primitive = root.join("hook-primitive");
    fs::create_dir_all(&primitive).unwrap();
    fs::write(
        primitive.join("coral.toml"),
        format!(
            r#"id = "{hook_id}"
version = "1.0.0"
type = "hook"
description = "A test hook."

[hook]
event = "before_finish"
command = "cargo test"
working_directory = "."
"#
        ),
    )
    .unwrap();
    primitive
}

#[test]
fn add_hook_installs_and_emits() {
    let temp = TempDir::new().unwrap();
    let hook = make_hook_primitive(temp.path(), "pre-commit");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["add", hook.to_str().unwrap(), "--target", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed pre-commit (open-agents) -> .agents/hooks/pre-commit/hook.toml",
        ))
        .stderr(predicate::str::contains(
            "this hook runs 'cargo test' on event 'before_finish'",
        ));

    assert!(temp
        .path()
        .join(".agents")
        .join("hooks")
        .join("pre-commit")
        .join("hook.toml")
        .exists());
}

#[test]
fn add_hook_rejects_invalid_event() {
    let temp = TempDir::new().unwrap();
    let primitive = temp.path().join("bad-hook");
    fs::create_dir_all(&primitive).unwrap();
    fs::write(
        primitive.join("coral.toml"),
        r#"id = "bad-hook"
version = "1.0.0"
type = "hook"
description = "Bad hook."

[hook]
event = ""
command = "echo test"
"#,
    )
    .unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--target", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("'event' must be a non-empty string"));
}

#[test]
fn add_hook_existing_event_rejected_by_adapter() {
    let temp = TempDir::new().unwrap();
    let primitive = temp.path().join("bad-hook");
    fs::create_dir_all(&primitive).unwrap();
    fs::write(
        primitive.join("coral.toml"),
        r#"id = "bad-hook"
version = "1.0.0"
type = "hook"
description = "Bad hook."

[hook]
event = "on_mars_landing"
command = "echo hello"
"#,
    )
    .unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--target", "claude"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not support hook event 'on_mars_landing'"));
}

#[test]
fn hook_list_and_drift() {
    let temp = TempDir::new().unwrap();
    let hook = make_hook_primitive(temp.path(), "pre-commit");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", hook.to_str().unwrap(), "--target", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["list", "--type", "hook"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "pre-commit\t1.0.0\tproject\topen-agents\tclean\t.agents/hooks/pre-commit/hook.toml",
        ));

    fs::write(
        temp.path()
            .join(".agents")
            .join("hooks")
            .join("pre-commit")
            .join("hook.toml"),
        "event = \"after_save\"\ncommand = \"cargo test\"\n",
    )
    .unwrap();

    coral()
        .current_dir(temp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "pre-commit\t1.0.0\tproject\topen-agents\tmodified\t",
        ));

    coral()
        .current_dir(temp.path())
        .args(["diff", "pre-commit"])
        .assert()
        .success()
        .stdout(predicate::str::contains("-event = \"before_finish\""))
        .stdout(predicate::str::contains("+event = \"after_save\""));
}

#[test]
fn hook_remove_cleans_directory() {
    let temp = TempDir::new().unwrap();
    let hook = make_hook_primitive(temp.path(), "pre-commit");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", hook.to_str().unwrap(), "--target", "claude"])
        .assert()
        .success();

    assert!(temp
        .path()
        .join(".claude")
        .join("hooks")
        .join("pre-commit")
        .join("hook.json")
        .exists());

    coral()
        .current_dir(temp.path())
        .args(["remove", "pre-commit"])
        .assert()
        .success();

    assert!(!temp
        .path()
        .join(".claude")
        .join("hooks")
        .join("pre-commit")
        .exists());
}
