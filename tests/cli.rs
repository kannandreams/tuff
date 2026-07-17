use std::{fs, path::Path, process::Command};

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

fn make_git_skill_repo(root: &Path) -> std::path::PathBuf {
    let repo = root.join("skill-repo");
    fs::create_dir_all(repo.join("skills").join("test-skill")).unwrap();
    fs::create_dir_all(repo.join("skills").join("test-skill").join("references")).unwrap();
    fs::write(
        repo.join("skills").join("test-skill").join("SKILL.md"),
        "# Test Skill\n\nHello from git-installed skill.\n",
    )
    .unwrap();
    fs::write(
        repo.join("skills")
            .join("test-skill")
            .join("references")
            .join("extra.md"),
        "# Extra reference\n",
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

fn test_fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
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
required = ["agent"]

[parameters.properties.endpoint]
type = "string"
description = "The endpoint to scan"

[implementation]
language = "bash"
entrypoint = "run.sh"
runtime_deps = ["curl"]
"#
        ),
    )
    .unwrap();
    fs::write(
        primitive.join("run.sh"),
        "#!/bin/bash\necho \"scanning: $1\"\n",
    )
    .unwrap();
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
fn malformed_manifest_fixture_is_rejected() {
    let temp = TempDir::new().unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args([
            "add",
            test_fixture("malformed-manifest").to_str().unwrap(),
            "--agent",
            "open-agents",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("missing field `description`"));
}

#[test]
fn legacy_lockfile_fixture_is_rejected() {
    let temp = TempDir::new().unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    fs::copy(
        test_fixture("legacy-lockfile").join("coral-lock.json"),
        temp.path().join(".coral").join("coral-lock.json"),
    )
    .unwrap();

    coral()
        .current_dir(temp.path())
        .args([
            "add",
            test_fixture("duplicate-files").to_str().unwrap(),
            "--agent",
            "open-agents",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported lockfile version: 2"));
}

#[test]
fn duplicate_files_fixture_installs_one_emitted_file() {
    let temp = TempDir::new().unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args([
            "add",
            test_fixture("duplicate-files").to_str().unwrap(),
            "--agent",
            "open-agents",
        ])
        .assert()
        .success();

    let installed = temp
        .path()
        .join(".agents")
        .join("skills")
        .join("duplicate-files")
        .join("SKILL.md");
    assert!(installed.exists());
    assert_eq!(
        fs::read_dir(installed.parent().unwrap()).unwrap().count(),
        1
    );
}

#[test]
fn invalid_capability_fixture_is_rejected() {
    let temp = TempDir::new().unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args([
            "add",
            test_fixture("invalid-capability").to_str().unwrap(),
            "--agent",
            "open-agents",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported capability type"));
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
        .stdout(predicate::str::contains(
            "initialized .coral/coral-lock.json",
        ));
    assert!(temp.path().join(".coral").join("coral-lock.json").exists());

    coral()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--agent", "open-agents"])
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
        .stdout(predicate::str::contains("│ ID"))
        .stdout(predicate::str::contains("example"))
        .stdout(predicate::str::contains("skill"))
        .stdout(predicate::str::contains("1.0.0"))
        .stdout(predicate::str::contains("project"))
        .stdout(predicate::str::contains("open-agents"))
        .stdout(predicate::str::contains("clean"))
        .stdout(predicate::str::contains(".agents/skills/example/SKILL.md"));

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
        .stdout(predicate::str::contains("example"))
        .stdout(predicate::str::contains("modified"))
        .stdout(predicate::str::contains(".agents/skills/example/SKILL.md"));

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
        .args(["add", primitive.to_str().unwrap(), "--agent", "open-agents"])
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
        .args(["add", primitive.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("refusing to overwrite untracked"));
}

#[test]
fn rejects_unknown_agent() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "example");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--agent", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown agent"));
}

#[test]
fn old_target_command_is_removed() {
    coral()
        .args(["target", "list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand 'target'"));
}

#[test]
fn old_target_flags_are_removed() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "old-target-flag");

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
        .stderr(predicate::str::contains("unexpected argument '--target'"));

    coral()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "-t", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument '-t'"));
}

#[test]
fn agent_list_add_remove() {
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
        .args(["agent", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("REGISTERED"))
        .stdout(predicate::str::contains("open-agents"))
        .stdout(predicate::str::contains("yes"))
        .stdout(predicate::str::contains("claude"));

    // Register Claude; Open Agents is registered by coral init.
    coral()
        .current_dir(temp.path())
        .args(["agent", "add", "claude"])
        .assert()
        .success()
        .stdout(predicate::str::contains("registered agent 'claude'"));

    assert!(temp.path().join(".claude").is_dir());

    // Install a skill to open-agents
    coral()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed example (open-agents) -> .agents/skills/example/SKILL.md",
        ));

    // Unregister open-agents without changing installed capabilities
    coral()
        .current_dir(temp.path())
        .args(["agent", "remove", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains("unregistered agent 'open-agents'"));

    assert!(
        temp.path()
            .join(".agents")
            .join("skills")
            .join("example")
            .join("SKILL.md")
            .exists()
    );
    coral()
        .current_dir(temp.path())
        .args(["list", "--scope", "project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("example"));
}

#[test]
fn agent_add_claude_creates_project_directory() {
    let temp = TempDir::new().unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["agent", "add", "claude"])
        .assert()
        .success()
        .stdout(predicate::str::contains("registered agent 'claude'"));

    assert!(temp.path().join(".claude").is_dir());
}

#[test]
fn configured_default_agent_is_used_when_agent_is_omitted() {
    let temp = TempDir::new().unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["agent", "set-default", "claude"])
        .assert()
        .success()
        .stdout(predicate::str::contains("set default agent 'claude'"));

    coral()
        .current_dir(temp.path())
        .args(["create", "skill", "defaulted-skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "created and tracked skill 'defaulted-skill' (claude)",
        ));

    assert!(
        temp.path()
            .join(".claude")
            .join("skills")
            .join("defaulted-skill")
            .join("SKILL.md")
            .exists()
    );
    assert!(
        !temp
            .path()
            .join(".agents")
            .join("skills")
            .join("defaulted-skill")
            .exists()
    );

    let config: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(temp.path().join(".coral").join("config.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(config["defaultAgent"], "claude");

    coral()
        .current_dir(temp.path())
        .args([
            "create",
            "skill",
            "explicit-open-agents",
            "-a",
            "open-agents",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "created and tracked skill 'explicit-open-agents' (open-agents)",
        ));
}

#[test]
fn global_default_agent_is_used_for_global_add() {
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let primitive = make_primitive(project.path(), "global-defaulted");

    coral()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["init", "--global"])
        .assert()
        .success();
    coral()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["agent", "set-default", "claude", "--global"])
        .assert()
        .success();

    coral()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["add", primitive.to_str().unwrap(), "--global"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed global-defaulted (claude)",
        ));

    assert!(
        home.path()
            .join(".claude")
            .join("skills")
            .join("global-defaulted")
            .join("SKILL.md")
            .exists()
    );
}

#[test]
fn add_to_multiple_agents() {
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
            "--agent",
            "open-agents",
            "--agent",
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
    assert!(
        temp.path()
            .join(".agents")
            .join("skills")
            .join("example")
            .join("SKILL.md")
            .exists()
    );
    assert!(
        temp.path()
            .join(".claude")
            .join("skills")
            .join("example")
            .join("SKILL.md")
            .exists()
    );

    // List should show both agents
    coral()
        .current_dir(temp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("example"))
        .stdout(predicate::str::contains("open-agents"))
        .stdout(predicate::str::contains(".agents/skills/example/SKILL.md"))
        .stdout(predicate::str::contains("claude"))
        .stdout(predicate::str::contains(".claude/skills/example/SKILL.md"));

    // Diff with specific agent
    coral()
        .current_dir(temp.path())
        .args(["diff", "example", "--agent", "open-agents"])
        .assert()
        .success();

    // Unregistering open-agents should keep all capability files
    coral()
        .current_dir(temp.path())
        .args(["agent", "remove", "open-agents"])
        .assert()
        .success();

    assert!(
        temp.path()
            .join(".claude")
            .join("skills")
            .join("example")
            .join("SKILL.md")
            .exists()
    );
    assert!(
        temp.path()
            .join(".agents")
            .join("skills")
            .join("example")
            .join("SKILL.md")
            .exists()
    );
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
            "--agent",
            "open-agents",
            "--skill",
            "test-skill",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed test-skill (open-agents) -> .agents/skills/test-skill/SKILL.md",
        ))
        .stdout(predicate::str::contains("references/extra.md").not());

    let installed_dir = temp
        .path()
        .join(".agents")
        .join("skills")
        .join("test-skill");
    assert!(installed_dir.join("SKILL.md").exists());
    assert!(installed_dir.join("coral.toml").exists());
    assert!(installed_dir.join("references").join("extra.md").exists());
    let generated_manifest = fs::read_to_string(installed_dir.join("coral.toml")).unwrap();
    assert!(generated_manifest.contains("id = \"test-skill\""));
    assert!(generated_manifest.contains("\"SKILL.md\""));
    assert!(generated_manifest.contains("\"references/extra.md\""));

    coral()
        .current_dir(temp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("test-skill"))
        .stdout(predicate::str::contains("clean"))
        .stdout(predicate::str::contains("references/extra.md").not());

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
        .stdout(predicate::str::contains("test-skill"))
        .stdout(predicate::str::contains("modified"));

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
        .args(["add", &repo_url, "--agent", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--skill, --tool, or --hook is required",
        ));
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
            "--agent",
            "open-agents",
            "--skill",
            "nonexistent",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn add_git_skill_multi_agent() {
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
            "--agent",
            "open-agents",
            "--agent",
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

    assert!(
        temp.path()
            .join(".agents")
            .join("skills")
            .join("test-skill")
            .join("SKILL.md")
            .exists()
    );
    assert!(
        temp.path()
            .join(".claude")
            .join("skills")
            .join("test-skill")
            .join("SKILL.md")
            .exists()
    );
}

#[test]
fn add_git_subfolder_skill() {
    let temp = TempDir::new().unwrap();
    let repo = temp.path().join("skill-repo");

    fs::create_dir_all(repo.join("skills").join("security").join("security-review")).unwrap();
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
            "--agent",
            "open-agents",
            "--skill",
            "security/security-review",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed security/security-review (open-agents) -> .agents/skills/security/security-review/SKILL.md",
        ));

    assert!(
        temp.path()
            .join(".agents")
            .join("skills")
            .join("security")
            .join("security-review")
            .join("SKILL.md")
            .exists()
    );
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
        .args(["add", primitive.to_str().unwrap(), "--agent", "codex"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed example (open-agents) -> .agents/skills/example/SKILL.md",
        ));

    assert!(
        temp.path()
            .join(".agents")
            .join("skills")
            .join("example")
            .join("SKILL.md")
            .exists()
    );
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
        .args(["add", primitive.to_str().unwrap(), "--agent", "claude-code"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed example (claude) -> .claude/skills/example/SKILL.md",
        ));

    assert!(
        temp.path()
            .join(".claude")
            .join("skills")
            .join("example")
            .join("SKILL.md")
            .exists()
    );
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
        .args([
            "add",
            primitive.to_str().unwrap(),
            "--agent",
            "open-agents",
            "--global",
        ])
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
        .args(["add", primitive.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("project"))
        .stdout(predicate::str::contains("VERSION"))
        .stdout(predicate::str::contains("example"))
        .stdout(predicate::str::contains("open-agents"))
        .stdout(predicate::str::contains("clean"));
}

#[test]
fn delete_generated_capability_cleans_up() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "remove-test");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["delete", "remove-test", "-a", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "deleted 'remove-test' from project scope",
        ));

    assert!(
        !temp
            .path()
            .join(".agents")
            .join("skills")
            .join("remove-test")
            .join("SKILL.md")
            .exists()
    );

    // Only check project scope — coral-cli-guide remains from init
    coral()
        .current_dir(temp.path())
        .args(["list", "--scope", "project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("coral-cli-guide"))
        .stdout(predicate::str::contains("remove-test").not());
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
    let _ = std::fs::remove_dir_all(
        home.join(".coral")
            .join("baselines")
            .join("open-agents")
            .join("dup-override"),
    );

    coral()
        .current_dir(temp.path())
        .args([
            "add",
            primitive_a.to_str().unwrap(),
            "--agent",
            "open-agents",
            "--global",
        ])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args([
            "add",
            primitive_a.to_str().unwrap(),
            "--agent",
            "open-agents",
        ])
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
            "--agent",
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
        .args(["add", tool.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed scan-tool (open-agents) -> .agents/tools/scan-tool/run.sh",
        ));

    assert!(
        temp.path()
            .join(".agents")
            .join("tools")
            .join("scan-tool")
            .join("run.sh")
            .exists()
    );
    assert!(
        temp.path()
            .join(".agents")
            .join("tools")
            .join("scan-tool")
            .join("coral.toml")
            .exists()
    );
    assert!(!temp.path().join(".agents").join("mcp.json").exists());
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
        .args(["add", primitive.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "parameters 'type' must be 'object'",
        ));
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
        .args(["add", primitive.to_str().unwrap(), "--agent", "open-agents"])
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
        .args(["add", tool.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "this tool requires runtime dependencies: curl",
        ));
}

#[test]
fn add_mcp_tool_registers_mcp_entry() {
    let temp = TempDir::new().unwrap();
    let tool = make_tool_primitive(temp.path(), "scan-tool");
    let manifest_path = tool.join("coral.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        manifest.replace(
            "entrypoint = \"run.sh\"",
            "entrypoint = \"run.sh\"\nmcp = true",
        ),
    )
    .unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["add", tool.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "registered MCP server scan-tool (open-agents) -> .agents/mcp.json",
        ));

    let mcp_path = temp.path().join(".agents").join("mcp.json");
    assert!(mcp_path.exists());
    let mcp: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&mcp_path).unwrap()).unwrap();
    assert!(mcp["mcpServers"]["scan-tool"].is_object());
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
        .args(["add", skill.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", tool.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["list", "--type", "tool"])
        .env_remove("NO_COLOR")
        .assert()
        .success()
        .stdout(predicate::str::contains("my-tool"))
        .stdout(predicate::str::contains("\u{1b}[35mtool\u{1b}[0m"))
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
fn add_tool_multi_agent_with_mcp() {
    let temp = TempDir::new().unwrap();
    let tool = make_tool_primitive(temp.path(), "scan-tool");
    let manifest_path = tool.join("coral.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        manifest.replace(
            "entrypoint = \"run.sh\"",
            "entrypoint = \"run.sh\"\nmcp = true",
        ),
    )
    .unwrap();

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
            "--agent",
            "open-agents",
            "--agent",
            "claude",
        ])
        .assert()
        .success();

    assert!(
        temp.path()
            .join(".agents")
            .join("tools")
            .join("scan-tool")
            .join("run.sh")
            .exists()
    );
    assert!(
        temp.path()
            .join(".claude")
            .join("tools")
            .join("scan-tool")
            .join("run.sh")
            .exists()
    );

    // Both MCP configs should exist
    let agents_mcp = temp.path().join(".agents").join("mcp.json");
    let claude_mcp = temp.path().join(".mcp.json");
    assert!(agents_mcp.exists());
    assert!(claude_mcp.exists());
}

#[test]
fn example_tools_install_and_register_mcp_entries() {
    let temp = TempDir::new().unwrap();
    let examples_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("tools");
    let examples = [
        ("local-binary-wrapper", "run.sh"),
        ("python-script-tool", "analyze_text.py"),
        ("mcp-server-tool", "server.js"),
        ("http-api-tool", "fetch_status.py"),
        ("repo-command-tool", "run_repo_check.sh"),
        ("docker-container-tool", "run_container.sh"),
    ];

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    for (id, entrypoint) in examples {
        let tool_dir = examples_root.join(id);
        coral()
            .current_dir(temp.path())
            .args(["add", tool_dir.to_str().unwrap()])
            .assert()
            .success()
            .stdout(predicate::str::contains(format!(
                "installed {id} (open-agents) -> .agents/tools/{id}/{entrypoint}"
            )));

        assert!(
            temp.path()
                .join(".agents")
                .join("tools")
                .join(id)
                .join(entrypoint)
                .exists()
        );
        assert!(
            temp.path()
                .join(".agents")
                .join("tools")
                .join(id)
                .join("coral.toml")
                .exists()
        );
    }

    let mcp_path = temp.path().join(".agents").join("mcp.json");
    let mcp: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&mcp_path).unwrap()).unwrap();
    assert!(mcp["mcpServers"]["mcp-server-tool"].is_object());
    for (id, _) in examples {
        if id != "mcp-server-tool" {
            assert!(
                mcp["mcpServers"][id].is_null(),
                "{id} should not be registered as an MCP server"
            );
        }
    }

    coral()
        .current_dir(temp.path())
        .args(["list", "--type", "tool"])
        .assert()
        .success()
        .stdout(predicate::str::contains("local-binary-wrapper"))
        .stdout(predicate::str::contains("python-script-tool"))
        .stdout(predicate::str::contains("mcp-server-tool"))
        .stdout(predicate::str::contains("http-api-tool"))
        .stdout(predicate::str::contains("repo-command-tool"))
        .stdout(predicate::str::contains("docker-container-tool"));
}

#[test]
fn remove_tool_cleans_mcp_entry() {
    let temp = TempDir::new().unwrap();
    let tool = make_tool_primitive(temp.path(), "scan-tool");
    let manifest_path = tool.join("coral.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        manifest.replace(
            "entrypoint = \"run.sh\"",
            "entrypoint = \"run.sh\"\nmcp = true",
        ),
    )
    .unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", tool.to_str().unwrap(), "--agent", "claude"])
        .assert()
        .success();

    let mcp_path = temp.path().join(".mcp.json");
    assert!(mcp_path.exists());
    let before: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&mcp_path).unwrap()).unwrap();
    assert!(before["mcpServers"]["scan-tool"].is_object());

    coral()
        .current_dir(temp.path())
        .args(["delete", "scan-tool", "-a", "claude"])
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
        .args(["add", hook.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed pre-commit (open-agents) -> .agents/hooks/pre-commit/hook.toml",
        ))
        .stderr(predicate::str::contains(
            "this hook runs 'cargo test' on event 'before_finish'",
        ));

    assert!(
        temp.path()
            .join(".agents")
            .join("hooks")
            .join("pre-commit")
            .join("hook.toml")
            .exists()
    );
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
        .args(["add", primitive.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "'event' must be a non-empty string",
        ));
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
        .args(["add", primitive.to_str().unwrap(), "--agent", "claude"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "does not support hook event 'on_mars_landing'",
        ));
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
        .args(["add", hook.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["list", "--type", "hook"])
        .assert()
        .success()
        .stdout(predicate::str::contains("pre-commit"))
        .stdout(predicate::str::contains("hook"))
        .stdout(predicate::str::contains(
            ".agents/hooks/pre-commit/hook.toml",
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
        .stdout(predicate::str::contains("pre-commit"))
        .stdout(predicate::str::contains("modified"));

    coral()
        .current_dir(temp.path())
        .args(["diff", "pre-commit"])
        .assert()
        .success()
        .stdout(predicate::str::contains("-event = \"before_finish\""))
        .stdout(predicate::str::contains("+event = \"after_save\""));
}

#[test]
fn delete_generated_hook_cleans_directory() {
    let temp = TempDir::new().unwrap();
    let hook = make_hook_primitive(temp.path(), "pre-commit");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", hook.to_str().unwrap(), "--agent", "claude"])
        .assert()
        .success();

    assert!(
        temp.path()
            .join(".claude")
            .join("hooks")
            .join("pre-commit")
            .join("hook.json")
            .exists()
    );

    coral()
        .current_dir(temp.path())
        .args(["delete", "pre-commit", "-a", "claude"])
        .assert()
        .success();

    assert!(
        !temp
            .path()
            .join(".claude")
            .join("hooks")
            .join("pre-commit")
            .exists()
    );
}

#[test]
fn outdated_reports_status() {
    let temp = TempDir::new().unwrap();
    let skill = make_primitive(temp.path(), "local-skill");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .arg("outdated")
        .assert()
        .success()
        .stdout(predicate::str::contains("│ ID"))
        .stdout(predicate::str::contains("local-skill"))
        .stdout(predicate::str::contains("up to date"))
        .stdout(predicate::str::contains("HEAD is now at").not());
}

#[test]
fn agent_add_already_registered_shows_message() {
    let temp = TempDir::new().unwrap();
    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["agent", "add", "claude"])
        .assert()
        .success();
    fs::remove_dir(temp.path().join(".claude")).unwrap();
    coral()
        .current_dir(temp.path())
        .args(["agent", "add", "claude"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already registered"));
    assert!(temp.path().join(".claude").is_dir());
}

#[test]
fn delete_with_agent_flag_only_removes_from_specified() {
    let temp = TempDir::new().unwrap();
    let skill = make_primitive(temp.path(), "target-test");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args([
            "add",
            skill.to_str().unwrap(),
            "--agent",
            "open-agents",
            "--agent",
            "claude",
        ])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["delete", "target-test", "-a", "open-agents"])
        .assert()
        .success();

    // Claude files should still exist
    assert!(
        temp.path()
            .join(".claude")
            .join("skills")
            .join("target-test")
            .join("SKILL.md")
            .exists()
    );
    assert!(
        !temp
            .path()
            .join(".agents")
            .join("skills")
            .join("target-test")
            .join("SKILL.md")
            .exists()
    );

    let lockfile: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(temp.path().join(".coral").join("coral-lock.json")).unwrap(),
    )
    .unwrap();
    assert!(lockfile["capabilities"]["target-test"]["targets"]["claude"].is_object());
    assert!(lockfile["capabilities"]["target-test"]["targets"]["open-agents"].is_null());
}

#[test]
fn outdated_no_primitives_shows_message() {
    let temp = TempDir::new().unwrap();
    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    // coral-cli-guide is auto-installed — verify it appears
    coral()
        .current_dir(temp.path())
        .arg("outdated")
        .assert()
        .success()
        .stdout(predicate::str::contains("coral-cli-guide"));
}

#[test]
fn diff_upstream_shows_no_changes_for_current_ref() {
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
            "--agent",
            "open-agents",
            "--skill",
            "test-skill",
        ])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["diff", "test-skill", "--upstream"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no upstream changes"));
}

#[test]
fn diff_upstream_error_on_local_primitive() {
    let temp = TempDir::new().unwrap();
    let skill = make_primitive(temp.path(), "local-only");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["diff", "local-only", "--upstream"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "upstream diff only available for git-sourced",
        ));
}

#[test]
fn check_clean_repo_reports_all_ok() {
    let temp = TempDir::new().unwrap();
    let skill = make_primitive(temp.path(), "check-skill");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .arg("check")
        .assert()
        .success()
        .stdout(predicate::str::contains("✓"))
        .stdout(predicate::str::contains("check-skill"))
        .stdout(predicate::str::contains("ok"));
}

#[test]
fn check_detects_modified_files() {
    let temp = TempDir::new().unwrap();
    let skill = make_primitive(temp.path(), "dirty-skill");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    fs::write(
        temp.path()
            .join(".agents")
            .join("skills")
            .join("dirty-skill")
            .join("SKILL.md"),
        "# Modified\n",
    )
    .unwrap();

    coral()
        .current_dir(temp.path())
        .arg("check")
        .assert()
        .failure()
        .stdout(predicate::str::contains("✗"))
        .stdout(predicate::str::contains("modified"));
}

#[test]
fn check_ignore_failures_exits_zero() {
    let temp = TempDir::new().unwrap();
    let skill = make_primitive(temp.path(), "dirty-skill");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    fs::write(
        temp.path()
            .join(".agents")
            .join("skills")
            .join("dirty-skill")
            .join("SKILL.md"),
        "# Modified\n",
    )
    .unwrap();

    coral()
        .current_dir(temp.path())
        .args(["check", "--ignore-failures"])
        .assert()
        .success()
        .stdout(predicate::str::contains("✗"));
}

#[test]
fn check_json_output() {
    let temp = TempDir::new().unwrap();
    let skill = make_primitive(temp.path(), "json-skill");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["check", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"valid\""));
}

#[test]
fn add_adopts_existing_agent_directory() {
    let temp = TempDir::new().unwrap();
    let skill_dir = temp.path().join(".agents").join("skills").join("my-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), "# Existing skill\n").unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["add", skill_dir.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "added my-skill (skill, open-agents)",
        ));

    assert!(skill_dir.join("coral.toml").exists());

    coral()
        .current_dir(temp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("my-skill"))
        .stdout(predicate::str::contains("skill"))
        .stdout(predicate::str::contains("clean"));
}

#[test]
fn generate_index_writes_default_open_agents_path() {
    let temp = TempDir::new().unwrap();
    let skill = make_primitive(temp.path(), "indexed-skill");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["generate", "index", "-a", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "generated index for open-agents -> .agents/CAPABILITIES.md",
        ));

    let index = fs::read_to_string(temp.path().join(".agents").join("CAPABILITIES.md")).unwrap();
    assert!(index.contains("# Capability Index: Open Agents"));
    assert!(index.contains("`indexed-skill`"));
    assert!(index.contains("Example capability."));
    assert!(index.contains(".agents/skills/indexed-skill/SKILL.md"));
    assert!(index.contains("`clean`"));
}

#[test]
fn generate_index_writes_default_claude_path() {
    let temp = TempDir::new().unwrap();
    let skill = make_primitive(temp.path(), "claude-indexed");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "-a", "claude"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["generate", "index", "-a", "claude"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "generated index for claude -> .claude/CAPABILITIES.md",
        ));

    let index = fs::read_to_string(temp.path().join(".claude").join("CAPABILITIES.md")).unwrap();
    assert!(index.contains("# Capability Index: Claude"));
    assert!(index.contains("`claude-indexed`"));
    assert!(index.contains(".claude/skills/claude-indexed/SKILL.md"));
}

#[test]
fn generate_index_supports_custom_output() {
    let temp = TempDir::new().unwrap();
    let skill = make_primitive(temp.path(), "custom-index");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args([
            "generate",
            "index",
            "-a",
            "open-agents",
            "--output",
            "docs/capabilities.md",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "generated index for open-agents -> docs/capabilities.md",
        ));

    let index = fs::read_to_string(temp.path().join("docs").join("capabilities.md")).unwrap();
    assert!(index.contains("`custom-index`"));
}

#[test]
fn generate_report_writes_project_report_with_status_summary() {
    let temp = TempDir::new().unwrap();
    let skill = make_primitive(temp.path(), "reported-skill");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success();

    fs::write(
        temp.path()
            .join(".agents")
            .join("skills")
            .join("reported-skill")
            .join("SKILL.md"),
        "# Example\n\nChanged text.\n",
    )
    .unwrap();

    coral()
        .current_dir(temp.path())
        .args(["generate", "report"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "generated report -> .coral/reports/coral-report.md",
        ));

    let report = fs::read_to_string(
        temp.path()
            .join(".coral")
            .join("reports")
            .join("coral-report.md"),
    )
    .unwrap();
    assert!(report.contains("# Coral Report"));
    assert!(report.contains("- Modified files: 1"));
    assert!(report.contains("`reported-skill`"));
    assert!(report.contains("`modified`"));
}

#[test]
fn generate_index_rejects_unknown_agent() {
    let temp = TempDir::new().unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["generate", "index", "-a", "unknown"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown agent 'unknown'"));
}

#[test]
fn untrack_in_place_capability_preserves_files() {
    let temp = TempDir::new().unwrap();
    let skill_dir = temp.path().join(".agents").join("skills").join("keep-me");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), "# Keep me\n").unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", skill_dir.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["delete", "keep-me", "-a", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("tracked in place"));
    assert!(skill_dir.join("SKILL.md").exists());

    coral()
        .current_dir(temp.path())
        .args(["untrack", "keep-me", "-a", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains("untracked 'keep-me'"));

    assert!(skill_dir.join("SKILL.md").exists());
    assert!(skill_dir.join("coral.toml").exists());
    assert!(
        !temp
            .path()
            .join(".coral")
            .join("baselines")
            .join("open-agents")
            .join("keep-me")
            .exists()
    );
}

#[test]
fn delete_requires_force_for_modified_generated_files() {
    let temp = TempDir::new().unwrap();
    let skill = make_primitive(temp.path(), "modified-delete");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success();

    fs::write(
        temp.path()
            .join(".agents")
            .join("skills")
            .join("modified-delete")
            .join("SKILL.md"),
        "# Local edit\n",
    )
    .unwrap();

    coral()
        .current_dir(temp.path())
        .args(["delete", "modified-delete", "-a", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("use --force to delete"));
    assert!(
        temp.path()
            .join(".agents")
            .join("skills")
            .join("modified-delete")
            .join("SKILL.md")
            .exists()
    );

    coral()
        .current_dir(temp.path())
        .args(["delete", "modified-delete", "-a", "open-agents", "--force"])
        .assert()
        .success();
    assert!(
        !temp
            .path()
            .join(".agents")
            .join("skills")
            .join("modified-delete")
            .exists()
    );
}

#[test]
fn create_skill_scaffolds_importable_files() {
    let temp = TempDir::new().unwrap();

    coral()
        .current_dir(temp.path())
        .args(["create", "skill", "my-skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "created and tracked skill 'my-skill'",
        ))
        .stdout(predicate::str::contains("coral list"));

    assert!(
        temp.path()
            .join(".agents")
            .join("skills")
            .join("my-skill")
            .join("coral.toml")
            .exists()
    );
    assert!(
        temp.path()
            .join(".agents")
            .join("skills")
            .join("my-skill")
            .join("SKILL.md")
            .exists()
    );
    assert!(temp.path().join(".coral").join("coral-lock.json").exists());

    coral()
        .current_dir(temp.path())
        .args(["list", "--scope", "project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("my-skill"))
        .stdout(predicate::str::contains("clean"));
}

#[test]
fn create_skill_can_select_claude_agent() {
    let temp = TempDir::new().unwrap();

    coral()
        .current_dir(temp.path())
        .args(["create", "skill", "my-skill", "--agent", "claude"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "created and tracked skill 'my-skill' (claude)",
        ))
        .stdout(predicate::str::contains(".claude/skills/my-skill/SKILL.md"));

    assert!(
        temp.path()
            .join(".claude")
            .join("skills")
            .join("my-skill")
            .join("SKILL.md")
            .exists()
    );

    let config: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(temp.path().join(".coral").join("config.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(config["agents"][0], "claude");
    assert_eq!(config["defaultAgent"], "open-agents");
}

#[test]
fn update_accepts_local_edits_as_new_baseline() {
    let temp = TempDir::new().unwrap();
    let skill = temp
        .path()
        .join(".agents")
        .join("skills")
        .join("local-skill")
        .join("SKILL.md");

    coral()
        .current_dir(temp.path())
        .args(["create", "skill", "local-skill"])
        .assert()
        .success();

    fs::write(&skill, "# Local Skill\n\nEdited locally.\n").unwrap();

    coral()
        .current_dir(temp.path())
        .args(["update", "local-skill", "--check"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "would record them as the new baseline",
        ));

    coral()
        .current_dir(temp.path())
        .args(["update", "local-skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains("updated local baseline"));

    coral()
        .current_dir(temp.path())
        .args(["diff", "local-skill"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    coral()
        .current_dir(temp.path())
        .args(["update", "local-skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already up to date"));
}

#[test]
fn update_accepts_adopted_local_edits() {
    let temp = TempDir::new().unwrap();
    let skill_dir = temp
        .path()
        .join(".agents")
        .join("skills")
        .join("existing-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), "# Existing\n").unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", skill_dir.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success();

    fs::write(skill_dir.join("SKILL.md"), "# Existing\n\nEdited.\n").unwrap();
    coral()
        .current_dir(temp.path())
        .args(["update", "existing-skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains("updated local baseline"));

    coral()
        .current_dir(temp.path())
        .args(["diff", "existing-skill"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn update_reloads_external_local_source() {
    let temp = TempDir::new().unwrap();
    let source = make_primitive(temp.path(), "external-skill");

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", source.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success();

    fs::write(
        source.join("src").join("SKILL.md"),
        "# Example\n\nUpdated from source.\n",
    )
    .unwrap();

    coral()
        .current_dir(temp.path())
        .args(["update", "external-skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed external-skill"));

    let installed = fs::read_to_string(
        temp.path()
            .join(".agents")
            .join("skills")
            .join("external-skill")
            .join("SKILL.md"),
    )
    .unwrap();
    assert!(installed.contains("Updated from source."));

    coral()
        .current_dir(temp.path())
        .args(["diff", "external-skill"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn update_local_can_select_one_agent() {
    let temp = TempDir::new().unwrap();
    let open_agents = temp
        .path()
        .join(".agents")
        .join("skills")
        .join("multi-skill")
        .join("SKILL.md");
    let claude = temp
        .path()
        .join(".claude")
        .join("skills")
        .join("multi-skill")
        .join("SKILL.md");

    coral()
        .current_dir(temp.path())
        .args([
            "create",
            "skill",
            "multi-skill",
            "-a",
            "open-agents",
            "-a",
            "claude",
        ])
        .assert()
        .success();

    fs::write(&open_agents, "# Open Agents edit\n").unwrap();
    fs::write(&claude, "# Claude edit\n").unwrap();
    coral()
        .current_dir(temp.path())
        .args(["update", "multi-skill", "-a", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["diff", "multi-skill", "-a", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    coral()
        .current_dir(temp.path())
        .args(["diff", "multi-skill", "-a", "claude"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Claude edit"));
}

#[test]
fn update_local_rejects_force_and_missing_files_without_partial_changes() {
    let temp = TempDir::new().unwrap();
    let skill_dir = temp
        .path()
        .join(".agents")
        .join("skills")
        .join("local-skill");
    let skill = skill_dir.join("SKILL.md");

    coral()
        .current_dir(temp.path())
        .args(["create", "skill", "local-skill"])
        .assert()
        .success();

    fs::write(&skill, "# Edited\n").unwrap();
    coral()
        .current_dir(temp.path())
        .args(["update", "local-skill", "--force"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("only valid for git-sourced"));

    fs::remove_file(&skill).unwrap();
    coral()
        .current_dir(temp.path())
        .args(["update", "local-skill"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("tracked file is missing"));

    let baseline = temp
        .path()
        .join(".coral")
        .join("baselines")
        .join("open-agents")
        .join("local-skill")
        .join("SKILL.md");
    assert!(!fs::read_to_string(baseline).unwrap().contains("Edited"));
}

#[test]
fn create_supports_multiple_agents_and_tracks_each_output() {
    let temp = TempDir::new().unwrap();

    coral()
        .current_dir(temp.path())
        .args([
            "create",
            "skill",
            "multi-skill",
            "--agent",
            "open-agents",
            "--agent",
            "claude",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("(open-agents)"))
        .stdout(predicate::str::contains("(claude)"));

    let lockfile: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(temp.path().join(".coral").join("coral-lock.json")).unwrap(),
    )
    .unwrap();
    assert!(lockfile["capabilities"]["multi-skill"]["targets"]["open-agents"].is_object());
    assert!(lockfile["capabilities"]["multi-skill"]["targets"]["claude"].is_object());
}

#[test]
fn create_generates_adapter_specific_hook_files() {
    let temp = TempDir::new().unwrap();

    coral()
        .current_dir(temp.path())
        .args(["create", "hook", "agents-hook"])
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["create", "hook", "claude-hook", "--agent", "claude"])
        .assert()
        .success();

    assert!(
        temp.path()
            .join(".agents/hooks/agents-hook/hook.toml")
            .exists()
    );
    let claude_hook = temp.path().join(".claude/hooks/claude-hook/hook.json");
    assert!(claude_hook.exists());
    let hook: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(claude_hook).unwrap()).unwrap();
    assert_eq!(hook["event"], "before_finish");
}

#[test]
fn create_rejects_legacy_flag_syntax() {
    let temp = TempDir::new().unwrap();

    coral()
        .current_dir(temp.path())
        .args(["create", "--skill", "legacy-skill"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument"));
}

#[test]
fn create_tool_scaffolds_executable_runner() {
    let temp = TempDir::new().unwrap();

    coral()
        .current_dir(temp.path())
        .args(["create", "tool", "scan-tool"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "created and tracked tool 'scan-tool'",
        ));

    let run_sh = temp
        .path()
        .join(".agents")
        .join("tools")
        .join("scan-tool")
        .join("run.sh");
    assert!(run_sh.exists());
    assert!(!temp.path().join(".agents").join("mcp.json").exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(run_sh).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111);
    }
}

#[test]
fn import_command_is_removed() {
    let temp = TempDir::new().unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .arg("import")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand 'import'"));
}

#[test]
fn add_rejects_already_tracked_agent_directory() {
    let temp = TempDir::new().unwrap();
    let skill_dir = temp.path().join(".agents").join("skills").join("dup");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), "# Dup\n").unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", skill_dir.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["add", skill_dir.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already tracked"));
}

#[test]
fn add_agent_directory_requires_matching_agent() {
    let temp = TempDir::new().unwrap();
    let skill_dir = temp
        .path()
        .join(".agents")
        .join("skills")
        .join("target-match");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), "# Target\n").unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["add", skill_dir.to_str().unwrap(), "-a", "claude"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("use -a open-agents"));
}

#[test]
fn add_does_not_batch_scan_agent_directories() {
    let temp = TempDir::new().unwrap();

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", "-a", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

fn make_workflow_primitive(
    root: &Path,
    wf_id: &str,
    req_ids: &[(&str, &str)],
) -> std::path::PathBuf {
    let primitive = root.join("wf-primitive");
    fs::create_dir_all(&primitive).unwrap();
    let mut content = format!(
        r#"id = "{wf_id}"
version = "1.0.0"
type = "workflow"
description = "A test workflow."
"#
    );
    for (rid, rtype) in req_ids {
        content.push_str(&format!(
            "[[workflow.requires]]\nid = \"{rid}\"\ntype = \"{rtype}\"\n"
        ));
    }
    fs::write(primitive.join("coral.toml"), content).unwrap();
    primitive
}

#[test]
fn add_workflow_installs_and_shows_deps() {
    let temp = TempDir::new().unwrap();
    let wf = make_workflow_primitive(
        temp.path(),
        "test-wf",
        &[("dep-a", "skill"), ("dep-b", "tool")],
    );

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .args(["add", wf.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed test-wf (open-agents)"))
        .stderr(predicate::str::contains(
            "workflow 'test-wf' requires 2 capabilities",
        ))
        .stderr(predicate::str::contains("dep-a (skill)"))
        .stderr(predicate::str::contains("dep-b (tool)"));

    assert!(
        temp.path()
            .join(".agents")
            .join("workflows")
            .join("test-wf")
            .join("workflow.toml")
            .exists()
    );

    coral()
        .current_dir(temp.path())
        .args(["list", "--type", "workflow"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test-wf"))
        .stdout(predicate::str::contains("workflow"));
}

#[test]
fn add_workflow_rejects_self_reference() {
    let temp = TempDir::new().unwrap();
    let wf_dir = temp.path().join("self-wf");
    fs::create_dir_all(&wf_dir).unwrap();
    fs::write(
        wf_dir.join("coral.toml"),
        r#"id = "self-wf"
version = "1.0.0"
type = "workflow"
description = "Bad."

[[workflow.requires]]
id = "self-wf"
type = "skill"
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
        .args(["add", wf_dir.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot require itself"));
}

#[test]
fn add_workflow_rejects_empty_requires() {
    let temp = TempDir::new().unwrap();
    let wf_dir = temp.path().join("empty-wf");
    fs::create_dir_all(&wf_dir).unwrap();
    fs::write(
        wf_dir.join("coral.toml"),
        r#"id = "empty-wf"
version = "1.0.0"
type = "workflow"
description = "Bad."

[workflow]
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
        .args(["add", wf_dir.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid capability manifest TOML"));
}

#[test]
fn add_workflow_rejects_duplicate_requires() {
    let temp = TempDir::new().unwrap();
    let wf_dir = temp.path().join("dup-wf");
    fs::create_dir_all(&wf_dir).unwrap();
    fs::write(
        wf_dir.join("coral.toml"),
        r#"id = "dup-wf"
version = "1.0.0"
type = "workflow"
description = "Bad."

[[workflow.requires]]
id = "same"
type = "skill"

[[workflow.requires]]
id = "same"
type = "tool"
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
        .args(["add", wf_dir.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("duplicate requirement"));
}

#[test]
fn status_shows_workflow_dependency_tree() {
    let temp = TempDir::new().unwrap();
    let skill = make_primitive(temp.path(), "dep-skill");
    let wf = make_workflow_primitive(temp.path(), "parent-wf", &[("dep-skill", "skill")]);

    coral()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();
    coral()
        .current_dir(temp.path())
        .args(["add", wf.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    coral()
        .current_dir(temp.path())
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("parent-wf"))
        .stdout(predicate::str::contains("dep-skill"));
}
