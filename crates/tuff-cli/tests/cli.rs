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
        .args(["config", "user.email", "test@tuff.test"])
        .current_dir(&repo)
        .status()
        .unwrap();
    assert!(status.success());

    let status = Command::new("git")
        .args(["config", "user.name", "Tuff Test"])
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
        primitive.join("tuff.toml"),
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

fn tuff() -> Command {
    Command::cargo_bin("tuff").unwrap()
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
        primitive.join("tuff.toml"),
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

fn make_pack(root: &Path) -> std::path::PathBuf {
    let pack = root.join("engineering-pack");
    let skill = pack.join("capabilities").join("pack-skill");
    let workflow = pack.join("capabilities").join("pack-workflow");
    fs::create_dir_all(&skill).unwrap();
    fs::create_dir_all(&workflow).unwrap();
    fs::write(
        pack.join("tuff-pack.toml"),
        r#"schema = 1
name = "com.acme/engineering"
version = "1.2.0"
description = "A deterministic test pack."

[build]
targets = ["open-agents"]

[[capabilities]]
path = "capabilities/pack-workflow"

[[capabilities]]
path = "capabilities/pack-skill"
"#,
    )
    .unwrap();
    fs::write(
        skill.join("tuff.toml"),
        r#"id = "pack-skill"
version = "2.0.0"
type = "skill"
description = "A skill shipped in a pack."
files = ["SKILL.md"]
"#,
    )
    .unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "# Pack skill\n\nPackaged guidance.\n",
    )
    .unwrap();
    fs::write(
        workflow.join("tuff.toml"),
        r#"id = "pack-workflow"
version = "3.0.0"
type = "workflow"
description = "A workflow shipped in a pack."

[[workflow.requires]]
id = "pack-skill"
type = "skill"
"#,
    )
    .unwrap();
    pack
}

fn make_runtime_pack(root: &Path) -> std::path::PathBuf {
    let pack = root.join("runtime-pack");
    let capabilities = pack.join("capabilities");
    fs::create_dir_all(&capabilities).unwrap();
    let tool = make_tool_primitive(&capabilities, "pack-mcp");
    let tool_manifest = fs::read_to_string(tool.join("tuff.toml")).unwrap();
    fs::write(
        tool.join("tuff.toml"),
        tool_manifest.replace(
            "entrypoint = \"run.sh\"",
            "entrypoint = \"run.sh\"\nmcp = true",
        ),
    )
    .unwrap();
    make_hook_primitive(&capabilities, "pack-hook");
    fs::write(
        pack.join("tuff-pack.toml"),
        r#"schema = 1
name = "com.acme/runtime"
version = "1.0.0"
description = "A pack with shared runtime configuration."

[build]
targets = ["open-agents"]

[[capabilities]]
path = "capabilities/tool-primitive"

[[capabilities]]
path = "capabilities/hook-primitive"
"#,
    )
    .unwrap();
    pack
}

#[test]
fn version_outputs_current_version() {
    tuff()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("tuff 0.5.0"));
}

#[test]
fn malformed_manifest_fixture_is_rejected() {
    let temp = TempDir::new().unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
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
    let home = TempDir::new().unwrap();

    tuff()
        .current_dir(temp.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .env("HOME", home.path())
        .args(["cache", "clear"])
        .assert()
        .success();
}

#[test]
fn duplicate_files_fixture_installs_one_emitted_file() {
    let temp = TempDir::new().unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            test_fixture("invalid-capability").to_str().unwrap(),
            "--agent",
            "open-agents",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown variant"));
}

#[test]
fn bare_command_outputs_welcome_menu() {
    tuff()
        .assert()
        .success()
        .stdout(predicate::str::contains("Tuff"))
        .stdout(predicate::str::contains(
            "is a capability lifecycle manager for coding agents.",
        ))
        .stdout(predicate::str::contains("tuff init"))
        .stdout(predicate::str::contains("tuff add"))
        .stdout(predicate::str::contains("tuff --help"));
}

#[test]
fn cli_lifecycle_reports_clean_modified_and_diff() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "example");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("initialized tuff.lock"));
    assert!(temp.path().join("tuff.lock").exists());

    tuff()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed example (open-agents) -> .agents/skills/example/SKILL.md",
        ));

    tuff()
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
        .stdout(predicate::str::contains(".agents/skills/example"));

    fs::write(
        temp.path()
            .join(".agents")
            .join("skills")
            .join("example")
            .join("SKILL.md"),
        "# Example\n\nChanged text.\n",
    )
    .unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("example"))
        .stdout(predicate::str::contains("modified"))
        .stdout(predicate::str::contains(".agents/skills/example"));

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("run 'tuff init' first"));
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--agent", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown agent"));
}

#[test]
fn old_target_command_is_removed() {
    tuff()
        .args(["target", "list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand 'target'"));
}

#[test]
fn old_target_flags_are_removed() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "old-target-flag");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
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
}

#[test]
fn agent_list_add_remove() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "example");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    // List available adapters
    tuff()
        .current_dir(temp.path())
        .args(["agent", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("REGISTERED"))
        .stdout(predicate::str::contains("open-agents"))
        .stdout(predicate::str::contains("yes"))
        .stdout(predicate::str::contains("claude"));

    // Register Claude; Open Agents is registered by tuff init.
    tuff()
        .current_dir(temp.path())
        .args(["agent", "add", "claude"])
        .assert()
        .success()
        .stdout(predicate::str::contains("registered agent 'claude'"));

    assert!(temp.path().join(".claude").is_dir());

    // Install a skill to open-agents
    tuff()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed example (open-agents) -> .agents/skills/example/SKILL.md",
        ));

    // Unregister open-agents without changing installed capabilities
    tuff()
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
    tuff()
        .current_dir(temp.path())
        .args(["list", "--scope", "project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("example"));
}

#[test]
fn agent_add_claude_creates_project_directory() {
    let temp = TempDir::new().unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args(["agent", "set-default", "claude"])
        .assert()
        .success()
        .stdout(predicate::str::contains("set default agent 'claude'"));

    tuff()
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

    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(temp.path().join("tuff.config.json")).unwrap())
            .unwrap();
    assert_eq!(config["defaultAgent"], "claude");

    tuff()
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

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["init", "--global"])
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["agent", "set-default", "claude", "--global"])
        .assert()
        .success();

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
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
    tuff()
        .current_dir(temp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("example"))
        .stdout(predicate::str::contains("open-agents"))
        .stdout(predicate::str::contains(".agents/skills/example"))
        .stdout(predicate::str::contains("claude"))
        .stdout(predicate::str::contains(".claude/skills/example"));

    // Diff with specific agent
    tuff()
        .current_dir(temp.path())
        .args(["diff", "example", "--agent", "open-agents"])
        .assert()
        .success();

    // Unregistering open-agents should keep all capability files
    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            "skill",
            &repo_url,
            "test-skill",
            "--agent",
            "open-agents",
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
    assert!(installed_dir.join("references").join("extra.md").exists());

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("test-skill"))
        .stdout(predicate::str::contains("modified"));

    tuff()
        .current_dir(temp.path())
        .args(["diff", "test-skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains("-Hello from git-installed skill."))
        .stdout(predicate::str::contains("+Changed from git source."));
}

#[test]
fn typed_add_rejects_parent_level_flags() {
    let temp = TempDir::new().unwrap();

    tuff()
        .current_dir(temp.path())
        .args(["add", "--agent", "claude", "tool", "./my-tool"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "for typed 'tuff add' commands, put --agent and --global after the capability source",
        ));
}

#[test]
fn add_git_requires_skill_flag() {
    let temp = TempDir::new().unwrap();
    let repo = make_git_skill_repo(temp.path());
    let repo_url = format!("file://{}", repo.display());

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args(["add", &repo_url, "--agent", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--name is required"));
}

#[test]
fn add_git_missing_skill_reports_error() {
    let temp = TempDir::new().unwrap();
    let repo = make_git_skill_repo(temp.path());
    let repo_url = format!("file://{}", repo.display());

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            "skill",
            &repo_url,
            "nonexistent",
            "--agent",
            "open-agents",
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            "skill",
            &repo_url,
            "test-skill",
            "--agent",
            "open-agents",
            "--agent",
            "claude",
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
        .args(["config", "user.email", "test@tuff.test"])
        .current_dir(&repo)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.name", "Tuff Test"])
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            "skill",
            &repo_url,
            "security/security-review",
            "--agent",
            "open-agents",
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
fn dedicated_codex_adapter_works() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "example");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--agent", "codex"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed example (codex) -> .agents/skills/example/SKILL.md",
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
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
    let home = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "global-skill");

    tuff()
        .current_dir(temp.path())
        .env("HOME", home.path())
        .args(["init", "--global"])
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .env("HOME", home.path())
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

    assert!(home.path().join(".local/state/tuff/tuff.lock").exists());
}

#[test]
fn check_global_excludes_modified_project_capabilities() {
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let primitive = make_primitive(project.path(), "project-skill");

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["init", "--global"])
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["add", primitive.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "add",
            primitive.to_str().unwrap(),
            "--name",
            "global-skill",
            "--agent",
            "open-agents",
            "--global",
        ])
        .assert()
        .success();

    fs::write(
        project.path().join(".agents/skills/project-skill/SKILL.md"),
        "# Modified\n",
    )
    .unwrap();

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["check", "--global"])
        .assert()
        .success()
        .stdout(predicate::str::contains("global-skill"))
        .stdout(predicate::str::contains("project-skill").not());

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("check")
        .assert()
        .failure()
        .stdout(predicate::str::contains("project-skill"))
        .stdout(predicate::str::contains("modified"));
}

#[test]
fn list_shows_scope_column() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "example");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    tuff()
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

    // Only check project scope — tuff-cli-guide remains from init
    tuff()
        .current_dir(temp.path())
        .args(["list", "--scope", "project"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tuff-cli-guide"))
        .stdout(predicate::str::contains("remove-test").not());
}

#[test]
fn status_shows_override_warning() {
    let temp = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let primitive_a = make_primitive(temp.path(), "dup-override");

    tuff()
        .current_dir(temp.path())
        .env("HOME", home.path())
        .args(["init", "--global"])
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .env("HOME", home.path())
        .args([
            "add",
            primitive_a.to_str().unwrap(),
            "--agent",
            "open-agents",
            "--global",
        ])
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            primitive_a.to_str().unwrap(),
            "--agent",
            "open-agents",
        ])
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .env("HOME", home.path())
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("dup-override"))
        .stdout(predicate::str::contains("[overrides global"))
        .stdout(predicate::str::contains("[shadowed by project copy]"));

    // Cleanup
}

#[test]
fn update_git_skill_reports_up_to_date() {
    let temp = TempDir::new().unwrap();
    let repo = make_git_skill_repo(temp.path());
    let repo_url = format!("file://{}", repo.display());

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            "skill",
            &repo_url,
            "test-skill",
            "--agent",
            "open-agents",
        ])
        .assert()
        .success();

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
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
    assert!(!temp.path().join(".agents").join("mcp.json").exists());
}

#[test]
fn local_add_name_overrides_manifest_id() {
    let temp = TempDir::new().unwrap();
    let primitive = make_primitive(temp.path(), "manifest-name");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            primitive.to_str().unwrap(),
            "--name",
            "installed-name",
            "--agent",
            "open-agents",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed installed-name"));

    assert!(
        temp.path()
            .join(".agents/skills/installed-name/SKILL.md")
            .is_file()
    );
    assert!(!temp.path().join(".agents/skills/manifest-name").exists());
    let lock = tuff_core::lockfile::read_lockfile_at(&temp.path().join("tuff.lock")).unwrap();
    assert!(lock.capabilities.contains_key("installed-name"));
    assert!(!lock.capabilities.contains_key("manifest-name"));
}

#[test]
fn add_tool_rejects_invalid_schema() {
    let temp = TempDir::new().unwrap();
    let primitive = temp.path().join("bad-tool");
    fs::create_dir_all(&primitive).unwrap();
    fs::write(
        primitive.join("tuff.toml"),
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
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
        primitive.join("tuff.toml"),
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
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
    let manifest_path = tool.join("tuff.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        manifest.replace(
            "entrypoint = \"run.sh\"",
            "entrypoint = \"run.sh\"\nmcp = true",
        ),
    )
    .unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
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
fn add_mcp_tool_rejects_malformed_config_before_writing_capability() {
    let temp = TempDir::new().unwrap();
    let tool = make_tool_primitive(temp.path(), "scan-tool");
    let manifest_path = tool.join("tuff.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        manifest.replace(
            "entrypoint = \"run.sh\"",
            "entrypoint = \"run.sh\"\nmcp = true",
        ),
    )
    .unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    let mcp_path = temp.path().join(".agents/mcp.json");
    fs::create_dir_all(mcp_path.parent().unwrap()).unwrap();
    let original = "{ malformed MCP config\n";
    fs::write(&mcp_path, original).unwrap();

    tuff()
        .current_dir(temp.path())
        .args(["add", tool.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid MCP config"))
        .stderr(predicate::str::contains(".agents/mcp.json"));

    assert_eq!(fs::read_to_string(&mcp_path).unwrap(), original);
    assert!(!temp.path().join(".agents/tools/scan-tool").exists());
    let lock = tuff_core::lockfile::read_lockfile_at(&temp.path().join("tuff.lock")).unwrap();
    assert!(!lock.capabilities.contains_key("scan-tool"));
}

#[test]
fn list_filter_by_primitive_kind() {
    let temp = TempDir::new().unwrap();
    let skill = make_primitive(temp.path(), "my-skill");
    let tool = make_tool_primitive(temp.path(), "my-tool");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", tool.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args(["list", "--type", "tool"])
        .env_remove("NO_COLOR")
        .assert()
        .success()
        .stdout(predicate::str::contains("my-tool"))
        .stdout(predicate::str::contains("\u{1b}[").not())
        .stdout(predicate::str::contains("my-skill").not());

    tuff()
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
    let manifest_path = tool.join("tuff.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        manifest.replace(
            "entrypoint = \"run.sh\"",
            "entrypoint = \"run.sh\"\nmcp = true",
        ),
    )
    .unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
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
        .join("../..")
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    for (id, entrypoint) in examples {
        let tool_dir = examples_root.join(id);
        tuff()
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

    tuff()
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
fn capability_index_reflects_installed_tool_and_updates_are_regenerated() {
    let temp = TempDir::new().unwrap();
    let tool = make_tool_primitive(temp.path(), "scan-tool");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args(["add", tool.to_str().unwrap()])
        .assert()
        .success();

    let index_path = temp
        .path()
        .join(".agents/skills/tuff-capabilities/SKILL.md");
    let content = fs::read_to_string(&index_path).unwrap();
    assert!(content.starts_with("---\nname: tuff-capabilities\n"));
    assert!(content.contains("### scan-tool — A test tool."));
    assert!(content.contains("Run: `bash .agents/tools/scan-tool/run.sh '<json-args>'`"));
    assert!(content.contains("- `endpoint` (string, optional): The endpoint to scan"));

    // A manifest edit picked up via `tuff update` regenerates the index too.
    let manifest_path = tool.join("tuff.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        manifest.replace("A test tool.", "An updated test tool."),
    )
    .unwrap();
    tuff()
        .current_dir(temp.path())
        .args(["update", "scan-tool", "--force"])
        .assert()
        .success();
    let updated = fs::read_to_string(&index_path).unwrap();
    assert!(updated.contains("### scan-tool — An updated test tool."));

    // Deleting the only indexed capability removes the whole index skill.
    tuff()
        .current_dir(temp.path())
        .args(["delete", "scan-tool", "--force"])
        .assert()
        .success();
    assert!(!index_path.exists());
    let lock = fs::read_to_string(temp.path().join("tuff.lock")).unwrap();
    assert!(!lock.contains("tuff-capabilities"));
}

#[test]
fn capability_index_lists_workflow_steps() {
    let temp = TempDir::new().unwrap();
    let workflow = make_workflow_primitive(temp.path(), "release-flow", &[("scan-tool", "tool")]);

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", workflow.to_str().unwrap()])
        .assert()
        .success();

    let content = fs::read_to_string(
        temp.path()
            .join(".agents/skills/tuff-capabilities/SKILL.md"),
    )
    .unwrap();
    assert!(content.contains("## Workflows"));
    assert!(content.contains("### release-flow"));
    assert!(content.contains("1. scan-tool (tool)"));
}

#[test]
fn capability_index_is_generated_for_a_pack_install() {
    let author = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let pack = make_pack(author.path());
    let artifact = author.path().join("pack.tuffpack");
    tuff()
        .current_dir(author.path())
        .args([
            "pack",
            "build",
            pack.to_str().unwrap(),
            "--output",
            artifact.to_str().unwrap(),
        ])
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "add",
            "pack",
            artifact.to_str().unwrap(),
            "--agent",
            "open-agents",
        ])
        .assert()
        .success();

    // The pack's own capability list never mentions the generated index, so
    // this proves `collect_install_mutations` carries its staged files
    // across to the real project (not just the lockfile entry).
    let index_path = project
        .path()
        .join(".agents/skills/tuff-capabilities/SKILL.md");
    assert!(index_path.is_file());
    let content = fs::read_to_string(&index_path).unwrap();
    assert!(content.contains("### pack-workflow"));
}

fn make_mcp_server_primitive(root: &Path, id: &str) -> std::path::PathBuf {
    let primitive = root.join("mcp-primitive");
    fs::create_dir_all(&primitive).unwrap();
    fs::write(
        primitive.join("tuff.toml"),
        format!(
            r#"id = "{id}"
version = "1.0.0"
type = "mcp-server"
description = "A test MCP server."

[server]
transport = "stdio"
command = "npx"
args = ["-y", "@example/server"]

[server.env]
EXAMPLE_TOKEN = {{ from_env = "EXAMPLE_TOKEN" }}

[server.metadata]
tools_summary = "do_thing, list_things"
"#
        ),
    )
    .unwrap();
    primitive
}

#[test]
fn add_mcp_from_local_path_emits_record_and_entry_and_delete_removes_both() {
    let temp = TempDir::new().unwrap();
    let server = make_mcp_server_primitive(temp.path(), "example");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", "mcp", server.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "registered MCP server example (open-agents) -> .agents/mcp.json",
        ))
        .stderr(predicate::str::contains("export EXAMPLE_TOKEN"));

    let record = temp.path().join(".agents/mcp-servers/example/server.toml");
    assert!(record.is_file());
    let record_text = fs::read_to_string(&record).unwrap();
    assert!(record_text.contains("type = \"mcp-server\""));
    assert!(record_text.contains("from_env = \"EXAMPLE_TOKEN\""));

    let mcp: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(temp.path().join(".agents/mcp.json")).unwrap())
            .unwrap();
    assert_eq!(mcp["mcpServers"]["example"]["command"], "npx");
    assert_eq!(
        mcp["mcpServers"]["example"]["env"]["EXAMPLE_TOKEN"],
        "${EXAMPLE_TOKEN}"
    );

    tuff()
        .current_dir(temp.path())
        .args(["list", "--type", "mcp-server"])
        .assert()
        .success()
        .stdout(predicate::str::contains("example"))
        .stdout(predicate::str::contains("mcp-server"));
    tuff()
        .current_dir(temp.path())
        .arg("check")
        .assert()
        .success();

    let index = fs::read_to_string(
        temp.path()
            .join(".agents/skills/tuff-capabilities/SKILL.md"),
    )
    .unwrap();
    assert!(index.contains("## MCP Servers"));
    assert!(index.contains("Tools: do_thing, list_things"));

    tuff()
        .current_dir(temp.path())
        .args(["delete", "example"])
        .assert()
        .success();
    assert!(!record.exists());
    let after: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(temp.path().join(".agents/mcp.json")).unwrap())
            .unwrap();
    assert!(after["mcpServers"]["example"].is_null());
    assert!(
        !fs::read_to_string(temp.path().join("tuff.lock"))
            .unwrap()
            .contains("name = \"example\"")
    );
}

#[test]
fn add_mcp_http_headers_install_in_every_harness_dialect_and_check_catches_edits() {
    let temp = TempDir::new().unwrap();
    let primitive = temp.path().join("http-primitive");
    fs::create_dir_all(&primitive).unwrap();
    fs::write(
        primitive.join("tuff.toml"),
        r#"id = "remote-example"
version = "1.0.0"
type = "mcp-server"
description = "A test remote MCP server."

[server]
transport = "http"
url = "https://mcp.example.test/mcp"

[server.headers]
Authorization = { from_env = "EXAMPLE_TOKEN", format = "Bearer {}" }
X-Api-Key = { from_env = "EXAMPLE_KEY" }
"#,
    )
    .unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    for agent in ["claude", "cursor"] {
        tuff()
            .current_dir(temp.path())
            .args(["agent", "add", agent])
            .assert()
            .success();
    }

    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            "mcp",
            primitive.to_str().unwrap(),
            "-a",
            "claude",
            "-a",
            "cursor",
            "-a",
            "open-agents",
        ])
        .assert()
        .success()
        // Both variables are named, whichever table references them.
        .stderr(predicate::str::contains(
            "export EXAMPLE_KEY, EXAMPLE_TOKEN",
        ));

    // Claude Code and Open Agents: `"type": "http"` and `${VAR}`.
    for path in [".mcp.json", ".agents/mcp.json"] {
        let config: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(temp.path().join(path)).unwrap()).unwrap();
        let entry = &config["mcpServers"]["remote-example"];
        assert_eq!(entry["type"], "http", "{path}");
        assert_eq!(entry["url"], "https://mcp.example.test/mcp", "{path}");
        assert_eq!(
            entry["headers"]["Authorization"], "Bearer ${EXAMPLE_TOKEN}",
            "{path}"
        );
        assert_eq!(entry["headers"]["X-Api-Key"], "${EXAMPLE_KEY}", "{path}");
    }

    // Cursor: no `type` for a remote server, and `${env:VAR}`.
    let cursor: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(temp.path().join(".cursor/mcp.json")).unwrap())
            .unwrap();
    let entry = &cursor["mcpServers"]["remote-example"];
    assert!(entry["type"].is_null(), "{entry}");
    assert_eq!(
        entry["headers"]["Authorization"],
        "Bearer ${env:EXAMPLE_TOKEN}"
    );
    assert_eq!(entry["headers"]["X-Api-Key"], "${env:EXAMPLE_KEY}");

    // The record keeps the declaration, references and all, never a value.
    let record = fs::read_to_string(
        temp.path()
            .join(".claude/mcp-servers/remote-example/server.toml"),
    )
    .unwrap();
    assert!(
        record.contains("[server.headers.Authorization]"),
        "{record}"
    );
    assert!(record.contains("format = \"Bearer {}\""), "{record}");

    tuff()
        .current_dir(temp.path())
        .arg("check")
        .assert()
        .success();

    // A hand-edited header is drift, exactly as a hand-edited command is.
    let path = temp.path().join(".mcp.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    config["mcpServers"]["remote-example"]["headers"]["Authorization"] =
        serde_json::Value::String("Bearer leaked-literal-token".to_string());
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("check")
        .assert()
        .failure()
        .stdout(predicate::str::contains("remote-example"))
        .stdout(predicate::str::contains("modified"));
}

#[test]
fn a_literal_mcp_header_value_is_refused_at_parse_time() {
    let temp = TempDir::new().unwrap();
    let primitive = temp.path().join("literal-header");
    fs::create_dir_all(&primitive).unwrap();
    fs::write(
        primitive.join("tuff.toml"),
        r#"id = "literal-header"
version = "1.0.0"
type = "mcp-server"
description = "A server declaring a secret inline."

[server]
transport = "http"
url = "https://mcp.example.test/mcp"

[server.headers]
Authorization = "Bearer sk-live-not-a-real-token"
"#,
    )
    .unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            "mcp",
            primitive.to_str().unwrap(),
            "-a",
            "open-agents",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("[server.headers]"))
        .stderr(predicate::str::contains("from_env"));
}

#[test]
fn add_mcp_from_catalog_never_prompts_or_hangs_non_interactively() {
    let temp = TempDir::new().unwrap();
    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    // assert_cmd pipes a closed stdin by default -- not a real terminal --
    // so this already exercises the non-interactive skip path without
    // --yes. Completing at all (assert_cmd applies its own generous
    // timeout) proves the prompt never blocked on a read; asserting no
    // prompt text on stdout proves it went to stderr as designed, not
    // wherever a script might capture and misinterpret it.
    tuff()
        .current_dir(temp.path())
        .args(["add", "mcp", "github", "-a", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains("reads GITHUB_PERSONAL_ACCESS_TOKEN").not());

    // --yes accepted explicitly too, and installs the same defaults.
    tuff()
        .current_dir(temp.path())
        .args(["add", "mcp", "notion", "-a", "open-agents", "--yes"])
        .assert()
        .success();
    let record =
        fs::read_to_string(temp.path().join(".agents/mcp-servers/notion/server.toml")).unwrap();
    assert!(record.contains("from_env = \"NOTION_TOKEN\""));
}

#[test]
fn add_mcp_from_catalog_wires_every_selected_harness_in_its_own_dialect() {
    let temp = TempDir::new().unwrap();
    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    for agent in ["claude", "cursor"] {
        tuff()
            .current_dir(temp.path())
            .args(["agent", "add", agent])
            .assert()
            .success();
    }

    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            "mcp",
            "github",
            "filesystem",
            "-a",
            "claude",
            "-a",
            "cursor",
            "-a",
            "open-agents",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed github from the built-in catalog",
        ))
        .stderr(predicate::str::contains(
            "export GITHUB_PERSONAL_ACCESS_TOKEN",
        ));

    // Claude Code and Open Agents expand `${VAR}`; Cursor needs `${env:VAR}`.
    let claude = fs::read_to_string(temp.path().join(".mcp.json")).unwrap();
    assert!(claude.contains("\"${GITHUB_PERSONAL_ACCESS_TOKEN}\""));
    let cursor = fs::read_to_string(temp.path().join(".cursor/mcp.json")).unwrap();
    assert!(cursor.contains("\"${env:GITHUB_PERSONAL_ACCESS_TOKEN}\""));
    let agents: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(temp.path().join(".agents/mcp.json")).unwrap())
            .unwrap();
    assert!(agents["mcpServers"]["github"].is_object());
    assert!(agents["mcpServers"]["filesystem"].is_object());
    for prefix in [".claude", ".cursor", ".agents"] {
        assert!(
            temp.path()
                .join(prefix)
                .join("mcp-servers/github/server.toml")
                .is_file()
        );
    }

    // The lockfile records the catalog as a typed source.
    let lock = fs::read_to_string(temp.path().join("tuff.lock")).unwrap();
    assert!(lock.contains("kind = \"catalog\""), "{lock}");
    assert!(lock.contains("id = \"github\""), "{lock}");

    tuff()
        .current_dir(temp.path())
        .arg("outdated")
        .assert()
        .success()
        .stdout(predicate::str::contains("up to date"));
    tuff()
        .current_dir(temp.path())
        .args(["update", "github"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already up to date"));
    tuff()
        .current_dir(temp.path())
        .arg("check")
        .assert()
        .success();
}

#[test]
fn hand_edited_mcp_entry_is_drift_gating_check_list_and_delete() {
    let temp = TempDir::new().unwrap();
    let server = make_mcp_server_primitive(temp.path(), "example");
    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", "mcp", server.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .arg("check")
        .assert()
        .success();

    // Tamper with the Tuff-managed entry, leaving a hand-written neighbour.
    let mcp_path = temp.path().join(".agents/mcp.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&mcp_path).unwrap()).unwrap();
    config["mcpServers"]["example"]["command"] = serde_json::json!("tampered");
    config["mcpServers"]["neighbour"] = serde_json::json!({"command": "hand-written"});
    fs::write(&mcp_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

    tuff()
        .current_dir(temp.path())
        .args(["list", "--type", "mcp-server"])
        .assert()
        .success()
        .stdout(predicate::str::contains("modified"));
    tuff()
        .current_dir(temp.path())
        .arg("check")
        .assert()
        .failure()
        .stdout(predicate::str::contains(".agents/mcp.json#example"));
    tuff()
        .current_dir(temp.path())
        .args(["delete", "example"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("local modifications"));

    // --force deletes the tampered entry but never touches the neighbour.
    tuff()
        .current_dir(temp.path())
        .args(["delete", "example", "--force"])
        .assert()
        .success();
    let after: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&mcp_path).unwrap()).unwrap();
    assert!(after["mcpServers"]["example"].is_null());
    assert_eq!(after["mcpServers"]["neighbour"]["command"], "hand-written");
}

#[test]
fn update_force_restores_a_tampered_catalog_entry() {
    let temp = TempDir::new().unwrap();
    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", "mcp", "github", "-a", "open-agents"])
        .assert()
        .success();

    let mcp_path = temp.path().join(".agents/mcp.json");
    let canonical = fs::read_to_string(&mcp_path).unwrap();
    let mut config: serde_json::Value = serde_json::from_str(&canonical).unwrap();
    config["mcpServers"]["github"]["command"] = serde_json::json!("tampered");
    fs::write(&mcp_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

    // The drifted entry blocks a plain update instead of hiding behind
    // "already up to date", and --check names the recovery.
    tuff()
        .current_dir(temp.path())
        .args(["update", "github", "--check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hand-edited MCP config entry"));
    tuff()
        .current_dir(temp.path())
        .args(["update", "github"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--force"));

    tuff()
        .current_dir(temp.path())
        .args(["update", "github", "--force"])
        .assert()
        .success();
    assert_eq!(fs::read_to_string(&mcp_path).unwrap(), canonical);
    tuff()
        .current_dir(temp.path())
        .arg("check")
        .assert()
        .success();
}

#[test]
fn add_mcp_refuses_an_untracked_entry_before_writing_anything() {
    let temp = TempDir::new().unwrap();
    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    let mcp_path = temp.path().join(".agents/mcp.json");
    let original = "{\n  \"mcpServers\": {\n    \"memory\": {\n      \"command\": \"hand-written\"\n    }\n  }\n}\n";
    fs::write(&mcp_path, original).unwrap();

    tuff()
        .current_dir(temp.path())
        .args(["add", "mcp", "memory", "-a", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "refusing to overwrite untracked MCP server 'memory'",
        ));

    assert_eq!(fs::read_to_string(&mcp_path).unwrap(), original);
    assert!(!temp.path().join(".agents/mcp-servers/memory").exists());
    assert!(
        !fs::read_to_string(temp.path().join("tuff.lock"))
            .unwrap()
            .contains("name = \"memory\"")
    );
}

#[test]
fn add_mcp_fails_closed_on_malformed_config() {
    let temp = TempDir::new().unwrap();
    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    let mcp_path = temp.path().join(".agents/mcp.json");
    fs::write(&mcp_path, "{ malformed\n").unwrap();

    tuff()
        .current_dir(temp.path())
        .args(["add", "mcp", "github", "-a", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid MCP config"));

    assert_eq!(fs::read_to_string(&mcp_path).unwrap(), "{ malformed\n");
    assert!(!temp.path().join(".agents/mcp-servers/github").exists());
}

#[test]
fn add_mcp_rejects_literal_env_values_and_unknown_ids() {
    let temp = TempDir::new().unwrap();
    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    let leaky = temp.path().join("leaky");
    fs::create_dir_all(&leaky).unwrap();
    fs::write(
        leaky.join("tuff.toml"),
        r#"id = "leaky"
version = "1.0.0"
type = "mcp-server"
description = "Puts a secret in the manifest."

[server]
command = "npx"

[server.env]
TOKEN = "ghp_literal_secret"
"#,
    )
    .unwrap();
    tuff()
        .current_dir(temp.path())
        .args(["add", "mcp", leaky.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("from_env"));
    assert!(!temp.path().join(".agents/mcp-servers/leaky").exists());

    // A name that is neither a built-in id nor in the registry. The stub
    // keeps this offline and deterministic; pointing at the real registry
    // would make the suite depend on the network and on what is published.
    let registry = StubRegistry::start(r#"{"servers":[]}"#);
    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            "mcp",
            "not-a-real-server",
            "--registry",
            &registry.url(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "is not a path, a git URL, a built-in catalog id, or a server in the MCP registry",
        ))
        .stderr(predicate::str::contains(
            "hint: run 'tuff mcp search not-a-real-server'",
        ));

    tuff()
        .current_dir(temp.path())
        .args(["create", "mcp-server", "x"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("use `tuff add mcp"));
}

fn make_local_mcp_server(root: &Path, id: &str, extra_toml: &str) -> std::path::PathBuf {
    let primitive = root.join("doctor-server-primitive");
    fs::create_dir_all(&primitive).unwrap();
    let script = test_fixture("mcp-doctor-server.js");
    fs::write(
        primitive.join("tuff.toml"),
        format!(
            r#"id = "{id}"
version = "1.0.0"
type = "mcp-server"
description = "A local, network-free MCP server for doctor tests."

[server]
transport = "stdio"
command = "node"
args = ["{}"]
{extra_toml}
"#,
            script.to_str().unwrap().replace('\\', "\\\\")
        ),
    )
    .unwrap();
    primitive
}

#[test]
fn mcp_doctor_reports_ok_and_lists_tools_for_a_healthy_server() {
    let temp = TempDir::new().unwrap();
    let server = make_local_mcp_server(temp.path(), "doctor-ok", "");
    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", "mcp", server.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args(["mcp", "doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("doctor-ok"))
        .stdout(predicate::str::contains("ok"))
        .stdout(predicate::str::contains("2 tool(s)"));

    tuff()
        .current_dir(temp.path())
        .args(["mcp", "doctor", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"echo\""))
        .stdout(predicate::str::contains("\"ping\""));
}

#[test]
fn mcp_doctor_reports_missing_env_without_spawning() {
    let temp = TempDir::new().unwrap();
    let server = make_local_mcp_server(
        temp.path(),
        "doctor-env",
        "\n[server.env]\nDOCTOR_TEST_TOKEN = { from_env = \"DOCTOR_TEST_TOKEN_UNSET\" }\n",
    );
    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .env_remove("DOCTOR_TEST_TOKEN_UNSET")
        .args(["add", "mcp", server.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .env_remove("DOCTOR_TEST_TOKEN_UNSET")
        .args(["mcp", "doctor"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("missing env"))
        .stdout(predicate::str::contains("DOCTOR_TEST_TOKEN_UNSET"));
}

#[test]
fn mcp_doctor_reports_spawn_failed_for_a_nonexistent_command() {
    let temp = TempDir::new().unwrap();
    let primitive = temp.path().join("bad-server-primitive");
    fs::create_dir_all(&primitive).unwrap();
    fs::write(
        primitive.join("tuff.toml"),
        r#"id = "doctor-bad-command"
version = "1.0.0"
type = "mcp-server"
description = "Points at a command that does not exist."

[server]
transport = "stdio"
command = "tuff-doctor-test-nonexistent-binary"
"#,
    )
    .unwrap();
    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            "mcp",
            primitive.to_str().unwrap(),
            "-a",
            "open-agents",
        ])
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args(["mcp", "doctor"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("spawn failed"));
}

#[test]
fn mcp_doctor_times_out_on_a_server_that_never_responds() {
    let temp = TempDir::new().unwrap();
    let server = make_local_mcp_server(temp.path(), "doctor-slow", "");
    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", "mcp", server.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .env("MCP_DOCTOR_TEST_DELAY_MS", "3000")
        .args(["mcp", "doctor", "--timeout", "1"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("timeout"));

    // --ignore-failures turns the same failure into a zero exit code.
    tuff()
        .current_dir(temp.path())
        .env("MCP_DOCTOR_TEST_DELAY_MS", "3000")
        .args(["mcp", "doctor", "--timeout", "1", "--ignore-failures"])
        .assert()
        .success()
        .stdout(predicate::str::contains("timeout"));
}

#[test]
fn remove_tool_cleans_mcp_entry() {
    let temp = TempDir::new().unwrap();
    let tool = make_tool_primitive(temp.path(), "scan-tool");
    let manifest_path = tool.join("tuff.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        manifest.replace(
            "entrypoint = \"run.sh\"",
            "entrypoint = \"run.sh\"\nmcp = true",
        ),
    )
    .unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", tool.to_str().unwrap(), "--agent", "claude"])
        .assert()
        .success();

    let mcp_path = temp.path().join(".mcp.json");
    assert!(mcp_path.exists());
    let before: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&mcp_path).unwrap()).unwrap();
    assert!(before["mcpServers"]["scan-tool"].is_object());

    tuff()
        .current_dir(temp.path())
        .args(["delete", "scan-tool", "-a", "claude"])
        .assert()
        .success();

    let after: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&mcp_path).unwrap()).unwrap();
    assert!(after["mcpServers"]["scan-tool"].is_null());
}

fn make_hook_primitive(root: &Path, hook_id: &str) -> std::path::PathBuf {
    make_hook_primitive_with_event(root, hook_id, "before_finish")
}

fn make_hook_primitive_with_event(root: &Path, hook_id: &str, event: &str) -> std::path::PathBuf {
    let primitive = root.join("hook-primitive");
    fs::create_dir_all(&primitive).unwrap();
    fs::write(
        primitive.join("tuff.toml"),
        format!(
            r#"id = "{hook_id}"
version = "1.0.0"
type = "hook"
description = "A test hook."

[hook]
event = "{event}"
command = "cargo test"
working_directory = "."
"#
        ),
    )
    .unwrap();
    primitive
}

fn make_multifile_hook(root: &Path, hook_id: &str) -> std::path::PathBuf {
    let primitive = root.join("multifile-hook");
    fs::create_dir_all(&primitive).unwrap();
    fs::write(
        primitive.join("tuff.toml"),
        format!(
            r#"id = "{hook_id}"
version = "1.0.0"
type = "hook"
description = "A multi-file test hook."
files = ["manifest.yaml", "script.py"]

[hook]
event = "before_finish"
command = "python3 .agents/hooks/{hook_id}/script.py"
"#
        ),
    )
    .unwrap();
    fs::write(
        primitive.join("manifest.yaml"),
        "name: original\nblocking: true\n",
    )
    .unwrap();
    fs::write(primitive.join("script.py"), "print('original')\n").unwrap();
    primitive
}

#[test]
fn add_hook_installs_and_emits() {
    let temp = TempDir::new().unwrap();
    let hook = make_hook_primitive(temp.path(), "pre-commit");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args(["add", hook.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed pre-commit (open-agents) -> .agents/hooks/pre-commit/run.sh",
        ))
        .stderr(predicate::str::contains(
            "this hook runs 'cargo test' on event 'before_finish'",
        ));

    assert!(
        temp.path()
            .join(".agents")
            .join("hooks")
            .join("pre-commit")
            .join("run.sh")
            .exists()
    );
}

#[test]
fn add_hook_renders_canonical_event_to_native_event() {
    let temp = TempDir::new().unwrap();
    let hook = make_hook_primitive_with_event(temp.path(), "tool-policy", "pre_tool_use");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args(["add", hook.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    let settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(temp.path().join(".agents/hook.json")).unwrap())
            .unwrap();
    assert!(
        settings["hooks"]["pre_tool_execution"].is_array(),
        "canonical pre_tool_use should render to open-agents pre_tool_execution"
    );
    assert!(settings["hooks"]["pre_tool_use"].is_null());

    let lock = tuff_core::lockfile::read_lockfile_at(&temp.path().join("tuff.lock")).unwrap();
    assert_eq!(
        lock.capabilities["tool-policy"].targets["open-agents"].managed_hooks[0]
            .canonical_event
            .as_deref(),
        Some("pre_tool_use")
    );
}

#[test]
fn claude_hook_matrix_renders_exact_native_event_names() {
    let temp = TempDir::new().unwrap();
    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    let cases = [
        ("session-start", "session_start"),
        ("session-end", "session_end"),
        ("pre-tool", "pre_tool_use"),
        ("post-tool", "post_tool_use"),
        ("before-finish", "before_finish"),
        ("stop", "stop"),
    ];
    for (id, canonical_event) in cases {
        let source_root = temp.path().join(format!("source-{id}"));
        let hook = make_hook_primitive_with_event(&source_root, id, canonical_event);
        tuff()
            .current_dir(temp.path())
            .args(["add", hook.to_str().unwrap(), "--agent", "claude"])
            .assert()
            .success();
    }

    let settings: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(temp.path().join(".claude/settings.json")).unwrap(),
    )
    .unwrap();
    let hooks = settings["hooks"].as_object().expect("Claude hooks object");
    for native_event in [
        "SessionStart",
        "SessionEnd",
        "PreToolUse",
        "PostToolUse",
        "Stop",
    ] {
        assert!(
            hooks
                .get(native_event)
                .is_some_and(|value| value.is_array()),
            "missing Claude native event {native_event}"
        );
    }
    assert_eq!(hooks.len(), 5, "unexpected Claude hook event: {hooks:?}");
    for obsolete_event in ["before_finish", "post_tool_execution", "pre_tool_execution"] {
        assert!(!hooks.contains_key(obsolete_event));
    }
}

#[test]
fn cursor_stop_uses_canonical_row_instead_of_before_finish_alias() {
    let temp = TempDir::new().unwrap();
    let hook = make_hook_primitive_with_event(temp.path(), "stop-policy", "stop");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", hook.to_str().unwrap(), "--agent", "cursor"])
        .assert()
        .success()
        .stderr(predicate::str::contains("partial compatibility").not());

    let lock = tuff_core::lockfile::read_lockfile_at(&temp.path().join("tuff.lock")).unwrap();
    let managed = &lock.capabilities["stop-policy"].targets["cursor"].managed_hooks[0];
    assert_eq!(managed.canonical_event.as_deref(), Some("stop"));
    assert_eq!(managed.event, "stop");
}

#[test]
fn add_hook_renders_cursor_hooks_json_shape() {
    let temp = TempDir::new().unwrap();
    let hook = make_hook_primitive_with_event(temp.path(), "cursor-policy", "pre_tool_use");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args(["agent", "add", "cursor"])
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", hook.to_str().unwrap(), "--agent", "cursor"])
        .assert()
        .success();

    let settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(temp.path().join(".cursor/hooks.json")).unwrap())
            .unwrap();
    assert_eq!(settings["version"], 1);
    assert!(
        settings["hooks"]["preToolUse"][0]["command"]
            .as_str()
            .is_some_and(|command| command.contains(".cursor/hooks/cursor-policy/run.sh"))
    );
}

#[test]
fn multifile_hook_diff_uses_directory_tree_and_json_hashes() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let hook = make_multifile_hook(temp.path(), "blocking-hook");

    tuff()
        .current_dir(temp.path())
        .env("HOME", &home)
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .env("HOME", &home)
        .args(["add", hook.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    fs::write(
        temp.path()
            .join(".agents/hooks/blocking-hook/manifest.yaml"),
        "name: original\nblocking: false\n",
    )
    .unwrap();

    let output = tuff()
        .current_dir(temp.path())
        .env("HOME", &home)
        .args(["diff", "blocking-hook", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value[0]["changes"][0]["path"], "manifest.yaml");
    assert_eq!(value[0]["changes"][0]["status"], "modified");
    assert!(value[0]["changes"][0]["old_hash"].is_string());
    assert!(value[0]["changes"][0]["new_hash"].is_string());
    assert!(temp.path().join("tuff.lock").exists());
    assert!(!temp.path().join(".tuff").exists());
}

#[test]
fn diff_refetches_baseline_after_cache_is_deleted() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let skill = make_primitive(temp.path(), "cold-cache");

    tuff()
        .current_dir(temp.path())
        .env("HOME", &home)
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .env("HOME", &home)
        .args(["add", skill.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();
    fs::write(
        temp.path().join(".agents/skills/cold-cache/SKILL.md"),
        "# Changed\n",
    )
    .unwrap();
    fs::remove_dir_all(home.join(".cache/tuff")).unwrap();

    tuff()
        .current_dir(temp.path())
        .env("HOME", &home)
        .args(["diff", "cold-cache"])
        .assert()
        .success()
        .stdout(predicate::str::contains("SKILL.md"))
        .stdout(predicate::str::contains("Changed"));
}

#[test]
fn local_baseline_refetch_verifies_source_and_never_uses_live_tree() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let source = make_primitive(temp.path(), "local-source");

    tuff()
        .current_dir(temp.path())
        .env("HOME", &home)
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .env("HOME", &home)
        .args(["add", source.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    fs::remove_dir_all(home.join(".cache/tuff")).unwrap();
    fs::write(source.join("src/SKILL.md"), "# Changed source\n").unwrap();
    tuff()
        .current_dir(temp.path())
        .env("HOME", &home)
        .args(["diff", "local-source"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "recorded baseline verification failed",
        ));

    fs::remove_dir_all(&source).unwrap();
    tuff()
        .current_dir(temp.path())
        .env("HOME", &home)
        .args(["diff", "local-source"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("local source \""))
        .stderr(predicate::str::contains("is no longer available"));
}

#[test]
fn upstream_diff_refetches_source_after_cache_is_deleted() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let source = make_git_skill_repo(temp.path());
    let url = format!("file://{}", source.display());

    tuff()
        .current_dir(temp.path())
        .env("HOME", &home)
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .env("HOME", &home)
        .args(["add", "skill", &url, "test-skill", "--agent", "open-agents"])
        .assert()
        .success();
    fs::remove_dir_all(home.join(".cache/tuff")).unwrap();

    tuff()
        .current_dir(temp.path())
        .env("HOME", &home)
        .args(["diff", "test-skill", "--upstream"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no upstream changes"));
}

#[test]
fn cache_clear_is_safe_and_lockfile_is_deterministic() {
    let temp = TempDir::new().unwrap();
    let home = temp.path().join("home");
    let skill = make_primitive(temp.path(), "deterministic");

    tuff()
        .current_dir(temp.path())
        .env("HOME", &home)
        .arg("init")
        .assert()
        .success();
    let first = fs::read(temp.path().join("tuff.lock")).unwrap();
    tuff()
        .current_dir(temp.path())
        .env("HOME", &home)
        .arg("init")
        .assert()
        .success();
    assert_eq!(first, fs::read(temp.path().join("tuff.lock")).unwrap());

    tuff()
        .current_dir(temp.path())
        .env("HOME", &home)
        .args(["add", skill.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .env("HOME", &home)
        .args(["cache", "clear"])
        .assert()
        .success();
    assert!(!home.join(".cache/tuff").exists());
}

#[test]
fn init_reconstructs_project_targets_from_lockfile_without_tuff_directory() {
    let temp = TempDir::new().unwrap();
    tuff()
        .current_dir(temp.path())
        .args(["create", "skill", "clone-target", "-a", "claude"])
        .assert()
        .success();
    fs::remove_file(temp.path().join("tuff.config.json")).unwrap();
    assert!(!temp.path().join(".tuff").exists());

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(temp.path().join("tuff.config.json")).unwrap())
            .unwrap();
    assert!(
        config["agents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|agent| agent == "claude")
    );
}

#[test]
fn add_hook_file_merges_claude_settings_and_copies_external_assets() {
    let temp = TempDir::new().unwrap();
    let hook = temp.path().join("claude-session-start");
    fs::create_dir_all(&hook).unwrap();
    fs::write(
        hook.join("settings.json"),
        r#"{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "{{hook_dir}}/session-start.sh"
          }
        ]
      }
    ]
  }
}
"#,
    )
    .unwrap();
    fs::write(
        hook.join("session-start.sh"),
        "#!/usr/bin/env bash\necho start\n",
    )
    .unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            "hook",
            hook.to_str().unwrap(),
            "--agent",
            "claude",
            "--hook-file",
            "settings.json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed claude-session-start (claude) -> .claude/hooks/claude-session-start/session-start.sh",
        ))
        .stdout(predicate::str::contains(
            "installed claude-session-start (claude) -> .claude/settings.json",
        ));

    assert!(
        temp.path()
            .join(".claude/hooks/claude-session-start/session-start.sh")
            .exists()
    );
    let settings: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(temp.path().join(".claude/settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        settings["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        ".claude/hooks/claude-session-start/session-start.sh"
    );
}

#[test]
fn native_hook_file_bypasses_canonical_event_validation() {
    let temp = TempDir::new().unwrap();
    let hook = temp.path().join("native-special-hook");
    fs::create_dir_all(&hook).unwrap();
    fs::write(
        hook.join("settings.json"),
        r#"{
  "hooks": {
    "on_mars_landing": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "{{hook_dir}}/run.sh"
          }
        ]
      }
    ]
  }
}
"#,
    )
    .unwrap();
    fs::write(hook.join("run.sh"), "#!/usr/bin/env bash\necho native\n").unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            "hook",
            hook.to_str().unwrap(),
            "--agent",
            "claude",
            "--hook-file",
            "settings.json",
        ])
        .assert()
        .success();

    let settings: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(temp.path().join(".claude/settings.json")).unwrap(),
    )
    .unwrap();
    assert!(settings["hooks"]["on_mars_landing"].is_array());
}

#[test]
fn add_hook_file_adopts_assets_already_inside_harness() {
    let temp = TempDir::new().unwrap();
    let hook = temp.path().join(".claude/hooks/session-start");
    fs::create_dir_all(&hook).unwrap();
    fs::write(
        hook.join("settings.json"),
        r#"{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "{{hook_dir}}/session-start.sh"
          }
        ]
      }
    ]
  }
}
"#,
    )
    .unwrap();
    fs::write(
        hook.join("session-start.sh"),
        "#!/usr/bin/env bash\necho start\n",
    )
    .unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            "hook",
            ".claude/hooks/session-start",
            "--agent",
            "claude",
            "--hook-file",
            "settings.json",
        ])
        .assert()
        .success();

    let lockfile = fs::read_to_string(temp.path().join("tuff.lock")).unwrap();
    assert!(lockfile.contains("name = \"session-start\""));
    assert!(lockfile.contains("target = \"claude\""));
    assert!(lockfile.contains("installed_path = \".claude/hooks/session-start\""));
}

#[test]
fn add_hook_rejects_invalid_event() {
    let temp = TempDir::new().unwrap();
    let primitive = temp.path().join("bad-hook");
    fs::create_dir_all(&primitive).unwrap();
    fs::write(
        primitive.join("tuff.toml"),
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
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
        primitive.join("tuff.toml"),
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", primitive.to_str().unwrap(), "--agent", "claude"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "does not support hook event 'on_mars_landing'",
        ));
}

#[test]
fn hooks_matrix_lists_registered_adapter_compatibility() {
    let temp = TempDir::new().unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args(["hooks", "matrix"])
        .assert()
        .success()
        .stdout(predicate::str::contains("open-agents"))
        .stdout(predicate::str::contains("pre_tool_use"))
        .stdout(predicate::str::contains("pre_tool_execution"))
        .stdout(predicate::str::contains("unsupported"));
}

#[test]
fn hooks_check_portability_requires_registered_target() {
    let temp = TempDir::new().unwrap();
    let hook = make_hook_primitive(temp.path(), "pre-commit");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", hook.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args([
            "hooks",
            "check-portability",
            "pre-commit",
            "--target",
            "claude",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "agent 'claude' is not registered in this project",
        ));
}

#[test]
fn hooks_check_portability_reports_target_coverage() {
    let temp = TempDir::new().unwrap();
    let hook = make_hook_primitive(temp.path(), "pre-commit");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["agent", "add", "claude"])
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", hook.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args([
            "hooks",
            "check-portability",
            "pre-commit",
            "--target",
            "claude",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("before_finish"))
        .stdout(predicate::str::contains("claude"))
        .stdout(predicate::str::contains("partial"));
}

#[test]
fn hooks_check_portability_uses_canonical_event_for_different_native_formats() {
    let temp = TempDir::new().unwrap();
    let hook = make_hook_primitive_with_event(temp.path(), "tool-policy", "pre_tool_use");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["agent", "add", "cursor"])
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", hook.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args([
            "hooks",
            "check-portability",
            "tool-policy",
            "--target",
            "cursor",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("pre_tool_use"))
        .stdout(predicate::str::contains("preToolUse"))
        .stdout(predicate::str::contains("cursor"))
        .stdout(predicate::str::contains("full"));
}

#[test]
fn hook_list_and_drift() {
    let temp = TempDir::new().unwrap();
    let hook = make_hook_primitive(temp.path(), "pre-commit");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", hook.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args(["list", "--type", "hook"])
        .assert()
        .success()
        .stdout(predicate::str::contains("pre-commit"))
        .stdout(predicate::str::contains("hook"))
        .stdout(predicate::str::contains(".agents/hooks/pre-commit"));

    fs::write(
        temp.path()
            .join(".agents")
            .join("hooks")
            .join("pre-commit")
            .join("run.sh"),
        "#!/usr/bin/env bash\nset -euo pipefail\ncd \".\"\necho changed\n",
    )
    .unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("pre-commit"))
        .stdout(predicate::str::contains("modified"));

    tuff()
        .current_dir(temp.path())
        .args(["diff", "pre-commit"])
        .assert()
        .success()
        .stdout(predicate::str::contains("run.sh"));
}

#[test]
fn delete_generated_hook_cleans_directory() {
    let temp = TempDir::new().unwrap();
    let hook = make_hook_primitive(temp.path(), "pre-commit");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", hook.to_str().unwrap(), "--agent", "claude"])
        .assert()
        .success();

    assert!(
        temp.path()
            .join(".claude")
            .join("hooks")
            .join("pre-commit")
            .join("run.sh")
            .exists()
    );

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .arg("outdated")
        .assert()
        .success()
        .stdout(predicate::str::contains("│ ID"))
        .stdout(predicate::str::contains("local-skill"))
        // A locally-installed skill has no upstream to compare against, so the
        // row must say so. This previously asserted "up to date", which stated
        // a conclusion that was never reached.
        .stdout(predicate::str::contains("not checked"))
        .stdout(predicate::str::contains("up to date").not())
        .stdout(predicate::str::contains("HEAD is now at").not());
}

#[test]
fn agent_add_already_registered_shows_message() {
    let temp = TempDir::new().unwrap();
    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["agent", "add", "claude"])
        .assert()
        .success();
    fs::remove_dir(temp.path().join(".claude")).unwrap();
    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
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

    tuff()
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

    let lockfile = fs::read_to_string(temp.path().join("tuff.lock")).unwrap();
    assert!(lockfile.contains("name = \"target-test\""));
    assert!(lockfile.contains("target = \"claude\""));
    assert_eq!(lockfile.matches("name = \"target-test\"").count(), 1);
}

#[test]
fn outdated_no_primitives_shows_message() {
    let temp = TempDir::new().unwrap();
    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    // tuff-cli-guide is auto-installed — verify it appears
    tuff()
        .current_dir(temp.path())
        .arg("outdated")
        .assert()
        .success()
        .stdout(predicate::str::contains("tuff-cli-guide"));
}

#[test]
fn diff_upstream_shows_no_changes_for_current_ref() {
    let temp = TempDir::new().unwrap();
    let repo = make_git_skill_repo(temp.path());
    let repo_url = format!("file://{}", repo.display());

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            "skill",
            &repo_url,
            "test-skill",
            "--agent",
            "open-agents",
        ])
        .assert()
        .success();

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
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

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
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

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args(["add", skill_dir.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "added my-skill (skill, open-agents)",
        ));

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success();

    tuff()
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
    assert!(index.contains(".agents/skills/indexed-skill"));
    assert!(index.contains("`clean`"));
}

#[test]
fn generate_index_writes_default_claude_path() {
    let temp = TempDir::new().unwrap();
    let skill = make_primitive(temp.path(), "claude-indexed");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "-a", "claude"])
        .assert()
        .success();

    tuff()
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
    assert!(index.contains(".claude/skills/claude-indexed"));
}

#[test]
fn generate_index_supports_custom_output() {
    let temp = TempDir::new().unwrap();
    let skill = make_primitive(temp.path(), "custom-index");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success();

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
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

    tuff()
        .current_dir(temp.path())
        .args(["generate", "report"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "generated report -> tuff-report.md",
        ));

    let report = fs::read_to_string(temp.path().join("tuff-report.md")).unwrap();
    assert!(report.contains("# Tuff Report"));
    assert!(report.contains("- Modified files: 1"));
    assert!(report.contains("`reported-skill`"));
    assert!(report.contains("`modified`"));
}

#[test]
fn generate_index_rejects_unknown_agent() {
    let temp = TempDir::new().unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", skill_dir.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success();
    let tracked_content_hash = fs::read_to_string(temp.path().join("tuff.lock")).unwrap();

    tuff()
        .current_dir(temp.path())
        .args(["delete", "keep-me", "-a", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("tracked in place"));
    assert!(skill_dir.join("SKILL.md").exists());

    tuff()
        .current_dir(temp.path())
        .args(["untrack", "keep-me", "-a", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains("untracked 'keep-me'"));

    assert!(skill_dir.join("SKILL.md").exists());
    assert!(tracked_content_hash.contains("sha256 = "));
}

#[test]
fn delete_requires_force_for_modified_generated_files() {
    let temp = TempDir::new().unwrap();
    let skill = make_primitive(temp.path(), "modified-delete");

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
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

    tuff()
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

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .args(["create", "skill", "my-skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "created and tracked skill 'my-skill'",
        ))
        .stdout(predicate::str::contains("tuff list"));

    assert!(
        temp.path()
            .join(".agents")
            .join("skills")
            .join("my-skill")
            .join("SKILL.md")
            .exists()
    );
    assert!(temp.path().join("tuff.lock").exists());

    tuff()
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

    tuff()
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

    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(temp.path().join("tuff.config.json")).unwrap())
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

    tuff()
        .current_dir(temp.path())
        .args(["create", "skill", "local-skill"])
        .assert()
        .success();

    fs::write(&skill, "# Local Skill\n\nEdited locally.\n").unwrap();

    tuff()
        .current_dir(temp.path())
        .args(["update", "local-skill", "--check"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "would record them as the new baseline",
        ));

    tuff()
        .current_dir(temp.path())
        .args(["update", "local-skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains("updated local baseline"));

    tuff()
        .current_dir(temp.path())
        .args(["diff", "local-skill"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", skill_dir.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success();

    fs::write(skill_dir.join("SKILL.md"), "# Existing\n\nEdited.\n").unwrap();
    tuff()
        .current_dir(temp.path())
        .args(["update", "existing-skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains("updated local baseline"));

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", source.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success();

    fs::write(
        source.join("src").join("SKILL.md"),
        "# Example\n\nUpdated from source.\n",
    )
    .unwrap();

    tuff()
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

    tuff()
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

    tuff()
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
    tuff()
        .current_dir(temp.path())
        .args(["update", "multi-skill", "-a", "open-agents"])
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args(["diff", "multi-skill", "-a", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    tuff()
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

    tuff()
        .current_dir(temp.path())
        .args(["create", "skill", "local-skill"])
        .assert()
        .success();

    fs::write(&skill, "# Edited\n").unwrap();
    tuff()
        .current_dir(temp.path())
        .args(["update", "local-skill", "--force"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("only valid for git-sourced"));

    fs::remove_file(&skill).unwrap();
    tuff()
        .current_dir(temp.path())
        .args(["update", "local-skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains("updated local baseline"));

    assert!(
        fs::read_to_string(temp.path().join("tuff.lock"))
            .unwrap()
            .contains("sha256 = ")
    );
}

#[test]
fn create_supports_multiple_agents_and_tracks_each_output() {
    let temp = TempDir::new().unwrap();

    tuff()
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

    let lockfile = fs::read_to_string(temp.path().join("tuff.lock")).unwrap();
    assert!(lockfile.contains("target = \"open-agents\""));
    assert!(lockfile.contains("target = \"claude\""));
}

#[test]
fn create_generates_adapter_specific_hook_files() {
    let temp = TempDir::new().unwrap();

    tuff()
        .current_dir(temp.path())
        .args(["create", "hook", "agents-hook"])
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["create", "hook", "claude-hook", "--agent", "claude"])
        .assert()
        .success();

    assert!(
        temp.path()
            .join(".agents/hooks/agents-hook/run.sh")
            .exists()
    );
    let agents_settings: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(temp.path().join(".agents/hook.json")).unwrap())
            .unwrap();
    assert_eq!(
        agents_settings["hooks"]["before_finish"][0]["hooks"][0]["command"],
        "sh .agents/hooks/agents-hook/run.sh"
    );
    let claude_hook = temp.path().join(".claude/hooks/claude-hook/run.sh");
    assert!(claude_hook.exists());
    let settings: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(temp.path().join(".claude/settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        settings["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        "sh .claude/hooks/claude-hook/run.sh"
    );
}

#[test]
fn create_rejects_legacy_flag_syntax() {
    let temp = TempDir::new().unwrap();

    tuff()
        .current_dir(temp.path())
        .args(["create", "--skill", "legacy-skill"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument"));
}

#[test]
fn create_tool_scaffolds_executable_runner() {
    let temp = TempDir::new().unwrap();

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .arg("import")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand 'import'"));
}

#[test]
fn init_emits_the_guide_for_a_detected_harness() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("CLAUDE.md"), "# Project\n").unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed tuff-cli-guide (claude) -> .claude/skills/tuff-cli-guide/SKILL.md",
        ));

    assert!(
        temp.path()
            .join(".claude/skills/tuff-cli-guide/SKILL.md")
            .is_file()
    );
    assert!(
        temp.path()
            .join(".agents/skills/tuff-cli-guide/SKILL.md")
            .is_file()
    );

    let lock = tuff_core::lockfile::read_lockfile_at(&temp.path().join("tuff.lock")).unwrap();
    let targets = &lock.capabilities["tuff-cli-guide"].targets;
    assert!(targets.contains_key("claude"));
    assert!(targets.contains_key("open-agents"));

    tuff()
        .current_dir(temp.path())
        .arg("check")
        .assert()
        .success();
}

#[test]
fn init_detects_cursor_from_its_directory() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join(".cursor")).unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    assert!(
        temp.path()
            .join(".cursor/skills/tuff-cli-guide/SKILL.md")
            .is_file()
    );
}

#[test]
fn init_records_only_open_agents_without_a_detected_harness() {
    let temp = TempDir::new().unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    assert!(!temp.path().join(".claude").exists());
    assert!(!temp.path().join(".cursor").exists());

    let lock = tuff_core::lockfile::read_lockfile_at(&temp.path().join("tuff.lock")).unwrap();
    let targets = &lock.capabilities["tuff-cli-guide"].targets;
    assert_eq!(targets.len(), 1);
    assert!(targets.contains_key("open-agents"));
}

/// Codex writes the same `.agents/` root as open-agents, and its detector
/// matches the directory `init` itself creates, so it must never be recorded
/// as a second target for the same emitted file.
#[test]
fn init_does_not_record_codex_alongside_open_agents() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("AGENTS.md"), "# Agents\n").unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    let lock = tuff_core::lockfile::read_lockfile_at(&temp.path().join("tuff.lock")).unwrap();
    let targets = &lock.capabilities["tuff-cli-guide"].targets;
    assert!(!targets.contains_key("codex"));
    assert!(targets.contains_key("open-agents"));
}

#[test]
fn add_gives_a_tracked_capability_another_agent() {
    let temp = TempDir::new().unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args(["add", ".agents/skills/tuff-cli-guide", "--agent", "claude"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed tuff-cli-guide (claude) -> .claude/skills/tuff-cli-guide/SKILL.md",
        ));

    let lock = tuff_core::lockfile::read_lockfile_at(&temp.path().join("tuff.lock")).unwrap();
    let targets = &lock.capabilities["tuff-cli-guide"].targets;
    assert!(targets.contains_key("claude"));
    assert!(targets.contains_key("open-agents"));

    tuff()
        .current_dir(temp.path())
        .arg("check")
        .assert()
        .success();
}

/// Adding a harness says nothing about where a capability came from, so the
/// recorded git source and its resolved revision must survive the addition.
#[test]
fn adding_an_agent_preserves_a_git_source() {
    let temp = TempDir::new().unwrap();
    let repo = make_git_skill_repo(temp.path());
    let repo_url = format!("file://{}", repo.display());

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            "skill",
            &repo_url,
            "test-skill",
            "--agent",
            "open-agents",
        ])
        .assert()
        .success();

    let before = tuff_core::lockfile::read_lockfile_at(&temp.path().join("tuff.lock")).unwrap();
    let before_entry = before.capabilities["test-skill"].clone();

    tuff()
        .current_dir(temp.path())
        .args(["add", ".agents/skills/test-skill", "--agent", "claude"])
        .assert()
        .success();

    let after = tuff_core::lockfile::read_lockfile_at(&temp.path().join("tuff.lock")).unwrap();
    let after_entry = &after.capabilities["test-skill"];

    assert_eq!(after_entry.source, before_entry.source);
    assert_eq!(after_entry.version, before_entry.version);
    assert_eq!(after_entry.version_scheme, before_entry.version_scheme);
    assert_eq!(after_entry.description, before_entry.description);
    assert!(after_entry.targets.contains_key("claude"));
    assert!(after_entry.targets.contains_key("open-agents"));
}

#[test]
fn add_rejects_already_tracked_agent_directory() {
    let temp = TempDir::new().unwrap();
    let skill_dir = temp.path().join(".agents").join("skills").join("dup");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), "# Dup\n").unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", skill_dir.to_str().unwrap(), "-a", "open-agents"])
        .assert()
        .success();

    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .args(["add", skill_dir.to_str().unwrap(), "-a", "claude"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("use -a open-agents"));
}

#[test]
fn add_does_not_batch_scan_agent_directories() {
    let temp = TempDir::new().unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
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
    fs::write(primitive.join("tuff.toml"), content).unwrap();
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    tuff()
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

    tuff()
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
        wf_dir.join("tuff.toml"),
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
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
        wf_dir.join("tuff.toml"),
        r#"id = "empty-wf"
version = "1.0.0"
type = "workflow"
description = "Bad."

[workflow]
"#,
    )
    .unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
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
        wf_dir.join("tuff.toml"),
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
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

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", skill.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args(["add", wf.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    tuff()
        .current_dir(temp.path())
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("parent-wf"))
        .stdout(predicate::str::contains("dep-skill"));
}

#[test]
fn pack_build_is_deterministic_and_extracts_a_verified_target() {
    let temp = TempDir::new().unwrap();
    let pack = make_pack(temp.path());
    let left = temp.path().join("left.tuffpack");
    let right = temp.path().join("right.tuffpack");
    let extracted = temp.path().join("runtime");

    tuff()
        .current_dir(temp.path())
        .args(["pack", "check", pack.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("2 capabilities"));
    tuff()
        .current_dir(temp.path())
        .args([
            "pack",
            "build",
            pack.to_str().unwrap(),
            "--output",
            left.to_str().unwrap(),
        ])
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args([
            "pack",
            "build",
            pack.to_str().unwrap(),
            "--output",
            right.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert_eq!(fs::read(&left).unwrap(), fs::read(&right).unwrap());

    tuff()
        .current_dir(temp.path())
        .args(["pack", "verify", left.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("sha256:"));
    tuff()
        .current_dir(temp.path())
        .args([
            "pack",
            "extract",
            left.to_str().unwrap(),
            "--agent",
            "open-agents",
            "--output",
            extracted.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(
        extracted
            .join(".agents/skills/pack-skill/SKILL.md")
            .is_file()
    );
}

#[test]
fn project_pack_build_packages_tracked_capabilities_with_simple_defaults() {
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let skill = make_primitive(project.path(), "code-review");

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["add", skill.to_str().unwrap()])
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["pack", "build", "--name", "crm-integration"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "tuff-dist/crm-integration-0.1.0.tuffpack",
        ));

    let artifact = project
        .path()
        .join("tuff-dist/crm-integration-0.1.0.tuffpack");
    let output = tuff()
        .current_dir(project.path())
        .args(["pack", "inspect", artifact.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(metadata["capabilities"][0]["id"], "code-review");
    assert_eq!(metadata["capabilities"].as_array().unwrap().len(), 1);
}

#[test]
fn project_pack_build_packages_skills_tools_hooks_and_workflows() {
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let skill = make_primitive(project.path(), "review-skill");
    let tool = make_tool_primitive(project.path(), "review-tool");
    let hook = make_hook_primitive(project.path(), "review-hook");
    let workflow = make_workflow_primitive(
        project.path(),
        "review-flow",
        &[("review-skill", "skill"), ("review-tool", "tool")],
    );

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();
    for source in [&skill, &tool, &hook, &workflow] {
        tuff()
            .current_dir(project.path())
            .env("HOME", home.path())
            .args(["add", source.to_str().unwrap()])
            .assert()
            .success();
    }

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["pack", "build", "--name", "security-review"])
        .assert()
        .success();
    let pack = tuff_core::pack::read_artifact(
        &project
            .path()
            .join("tuff-dist/security-review-0.1.0.tuffpack"),
    )
    .unwrap();
    let types = pack
        .metadata
        .capabilities
        .iter()
        .map(|capability| capability.capability_type.as_str())
        .collect::<Vec<_>>();
    assert_eq!(types, ["workflow", "hook", "skill", "tool"]);
}

#[test]
fn project_pack_build_refuses_drift_without_writing_an_artifact() {
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let skill = make_primitive(project.path(), "code-review");

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["add", skill.to_str().unwrap()])
        .assert()
        .success();
    fs::write(
        project.path().join(".agents/skills/code-review/SKILL.md"),
        "# Unaccepted change\n",
    )
    .unwrap();

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["pack", "build", "--name", "crm-integration"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "selected capabilities have unaccepted changes",
        ))
        .stderr(predicate::str::contains("tuff update <capability>"));
    assert!(!project.path().join("tuff-dist").exists());
}

#[test]
fn project_pack_build_allows_explicit_guide_and_custom_version_and_agent() {
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "pack",
            "build",
            "--name",
            "guide",
            "--capability",
            "tuff-cli-guide",
            "--version",
            "2.4.0",
            "--agent",
            "claude",
        ])
        .assert()
        .success();

    let artifact = project.path().join("tuff-dist/guide-2.4.0.tuffpack");
    let pack = tuff_core::pack::read_artifact(&artifact).unwrap();
    assert_eq!(pack.metadata.capabilities[0].id, "tuff-cli-guide");
    assert_eq!(pack.metadata.targets[0].id, "claude");
}

#[test]
fn project_pack_build_refuses_source_changes_not_accepted_by_update() {
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let skill = make_primitive(project.path(), "code-review");

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["add", skill.to_str().unwrap()])
        .assert()
        .success();
    fs::write(skill.join("src/SKILL.md"), "# Changed source\n").unwrap();

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["pack", "build", "--name", "crm-integration"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "no longer reproduces its accepted 'open-agents' baseline",
        ))
        .stderr(predicate::str::contains("tuff update code-review"));
    assert!(!project.path().join("tuff-dist").exists());
}

#[test]
fn project_pack_init_persists_expanded_workflow_selection_without_copying_sources() {
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let skill = make_primitive(project.path(), "dep-skill");
    let workflow =
        make_workflow_primitive(project.path(), "review-flow", &[("dep-skill", "skill")]);

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();
    for source in [&skill, &workflow] {
        tuff()
            .current_dir(project.path())
            .env("HOME", home.path())
            .args(["add", source.to_str().unwrap()])
            .assert()
            .success();
    }
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "pack",
            "init",
            "crm-integration",
            "--from-project",
            "--capability",
            "review-flow",
        ])
        .assert()
        .success();

    let pack_root = project.path().join("tuff-packs/crm-integration");
    let (_, manifest) = tuff_core::pack::load_manifest(&pack_root).unwrap();
    assert_eq!(
        manifest.project.unwrap().capabilities,
        ["dep-skill", "review-flow"]
    );
    assert!(!pack_root.join("capabilities").exists());
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["pack", "build", pack_root.to_str().unwrap()])
        .assert()
        .success();
    assert!(
        project
            .path()
            .join("tuff-dist/crm-integration-0.1.0.tuffpack")
            .is_file()
    );
}

#[test]
fn project_pack_build_explains_unreconstructable_pack_installed_non_skill() {
    let author = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let source_pack = make_runtime_pack(author.path());
    let artifact = author.path().join("runtime.tuffpack");
    tuff()
        .current_dir(author.path())
        .args([
            "pack",
            "build",
            source_pack.to_str().unwrap(),
            "--output",
            artifact.to_str().unwrap(),
        ])
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["add", "pack", artifact.to_str().unwrap()])
        .assert()
        .success();

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "pack",
            "build",
            "--name",
            "runtime",
            "--capability",
            "pack-mcp",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "cannot reconstruct portable source for tool capability 'pack-mcp'",
        ))
        .stderr(predicate::str::contains(
            "reinstall it from a manifest-backed local source",
        ));
    assert!(!project.path().join("tuff-dist").exists());
}

#[test]
fn pack_verify_rejects_tampered_artifact() {
    let temp = TempDir::new().unwrap();
    let pack = make_pack(temp.path());
    let artifact = temp.path().join("pack.tuffpack");
    tuff()
        .current_dir(temp.path())
        .args([
            "pack",
            "build",
            pack.to_str().unwrap(),
            "--output",
            artifact.to_str().unwrap(),
        ])
        .assert()
        .success();
    let mut bytes = fs::read(&artifact).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    fs::write(&artifact, bytes).unwrap();

    tuff()
        .current_dir(temp.path())
        .args(["pack", "verify", artifact.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("hash mismatch"));
}

#[test]
fn pack_push_requires_an_explicit_tag_before_network_access() {
    tuff()
        .args([
            "pack",
            "push",
            "missing.tuffpack",
            "ghcr.io/acme/engineering",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("explicit tag"));
}

#[test]
fn pack_pull_rejects_implicit_latest_before_network_access() {
    let temp = TempDir::new().unwrap();
    tuff()
        .args([
            "pack",
            "pull",
            "ghcr.io/acme/engineering",
            "--output",
            temp.path().join("pack.tuffpack").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("implicit 'latest'"));
}

#[test]
fn pack_pull_refuses_existing_output_before_network_access() {
    let temp = TempDir::new().unwrap();
    let output = temp.path().join("pack.tuffpack");
    fs::write(&output, "keep me").unwrap();

    tuff()
        .args([
            "pack",
            "pull",
            "localhost:1/acme/engineering:1.2.0",
            "--output",
            output.to_str().unwrap(),
            "--plain-http",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("refusing to overwrite"));
    assert_eq!(fs::read_to_string(output).unwrap(), "keep me");
}

#[test]
fn pack_check_rejects_missing_workflow_dependency() {
    let temp = TempDir::new().unwrap();
    let pack = make_pack(temp.path());
    fs::write(
        pack.join("capabilities/pack-workflow/tuff.toml"),
        r#"id = "pack-workflow"
version = "3.0.0"
type = "workflow"
description = "A broken workflow."

[[workflow.requires]]
id = "missing-skill"
type = "skill"
"#,
    )
    .unwrap();

    tuff()
        .current_dir(temp.path())
        .args(["pack", "check", pack.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires missing capability"));
}

#[test]
fn add_pack_installs_all_members_and_records_provenance() {
    let author = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let pack = make_pack(author.path());
    let artifact = author.path().join("pack.tuffpack");
    tuff()
        .current_dir(author.path())
        .args([
            "pack",
            "build",
            pack.to_str().unwrap(),
            "--output",
            artifact.to_str().unwrap(),
        ])
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "add",
            "pack",
            artifact.to_str().unwrap(),
            "--agent",
            "open-agents",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed pack com.acme/engineering 1.2.0",
        ));

    let lock = fs::read_to_string(project.path().join("tuff.lock")).unwrap();
    assert!(lock.contains("name = \"com.acme/engineering\""));
    assert!(
        project
            .path()
            .join(".agents/skills/pack-skill/SKILL.md")
            .is_file()
    );
    assert!(
        project
            .path()
            .join(".agents/workflows/pack-workflow/workflow.toml")
            .is_file()
    );
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("pack-skill"))
        .stdout(predicate::str::contains("pack-workflow"));
}

#[test]
fn add_pack_collision_leaves_every_member_uninstalled() {
    let author = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let pack = make_pack(author.path());
    let artifact = author.path().join("pack.tuffpack");
    tuff()
        .current_dir(author.path())
        .args([
            "pack",
            "build",
            pack.to_str().unwrap(),
            "--output",
            artifact.to_str().unwrap(),
        ])
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();
    let collision = project.path().join(".agents/skills/pack-skill");
    fs::create_dir_all(&collision).unwrap();
    fs::write(collision.join("SKILL.md"), "user managed\n").unwrap();

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "add",
            "pack",
            artifact.to_str().unwrap(),
            "--agent",
            "open-agents",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("all-or-nothing"));

    assert!(
        !project
            .path()
            .join(".agents/workflows/pack-workflow")
            .exists()
    );
}

#[test]
fn add_pack_merges_hook_and_mcp_configuration_without_executing_members() {
    let author = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let pack = make_runtime_pack(author.path());
    let artifact = author.path().join("runtime.tuffpack");
    tuff()
        .current_dir(author.path())
        .args([
            "pack",
            "build",
            pack.to_str().unwrap(),
            "--output",
            artifact.to_str().unwrap(),
        ])
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();
    fs::write(
        project.path().join(".agents/mcp.json"),
        r#"{"custom":{"preserved":true}}"#,
    )
    .unwrap();

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "add",
            "pack",
            artifact.to_str().unwrap(),
            "--agent",
            "open-agents",
        ])
        .assert()
        .success();

    let mcp: serde_json::Value =
        serde_json::from_slice(&fs::read(project.path().join(".agents/mcp.json")).unwrap())
            .unwrap();
    assert_eq!(mcp["custom"]["preserved"], true);
    assert!(mcp["mcpServers"]["pack-mcp"].is_object());
    let hooks = fs::read_to_string(project.path().join(".agents/hook.json")).unwrap();
    assert!(hooks.contains("pack-hook"));
}

// ── pack updates ─────────────────────────────────────────────────────

/// A release of `com.acme/engineering` with a chosen membership, so two
/// releases can differ in content, drop a member, and add one.
fn make_pack_release(
    root: &Path,
    version: &str,
    skill_body: &str,
    with_workflow: bool,
    with_notes: bool,
) -> std::path::PathBuf {
    let pack = root.join(format!("engineering-{version}"));
    let skill = pack.join("capabilities/pack-skill");
    fs::create_dir_all(&skill).unwrap();
    let mut manifest = format!(
        r#"schema = 1
name = "com.acme/engineering"
version = "{version}"
description = "A versioned test pack."

[build]
targets = ["open-agents"]

[[capabilities]]
path = "capabilities/pack-skill"
"#
    );
    fs::write(
        skill.join("tuff.toml"),
        format!(
            r#"id = "pack-skill"
version = "{version}"
type = "skill"
description = "A skill shipped in a pack."
files = ["SKILL.md"]
"#
        ),
    )
    .unwrap();
    fs::write(skill.join("SKILL.md"), skill_body).unwrap();
    if with_workflow {
        let workflow = pack.join("capabilities/pack-workflow");
        fs::create_dir_all(&workflow).unwrap();
        fs::write(
            workflow.join("tuff.toml"),
            r#"id = "pack-workflow"
version = "1.0.0"
type = "workflow"
description = "A workflow shipped in a pack."

[[workflow.requires]]
id = "pack-skill"
type = "skill"
"#,
        )
        .unwrap();
        manifest.push_str("\n[[capabilities]]\npath = \"capabilities/pack-workflow\"\n");
    }
    if with_notes {
        let notes = pack.join("capabilities/pack-notes");
        fs::create_dir_all(&notes).unwrap();
        fs::write(
            notes.join("tuff.toml"),
            r#"id = "pack-notes"
version = "1.0.0"
type = "skill"
description = "A skill added in a later release."
files = ["SKILL.md"]
"#,
        )
        .unwrap();
        fs::write(notes.join("SKILL.md"), "# Notes\n").unwrap();
        manifest.push_str("\n[[capabilities]]\npath = \"capabilities/pack-notes\"\n");
    }
    fs::write(pack.join("tuff-pack.toml"), manifest).unwrap();
    let artifact = root.join(format!("engineering-{version}.tuffpack"));
    tuff()
        .current_dir(root)
        .args([
            "pack",
            "build",
            pack.to_str().unwrap(),
            "--output",
            artifact.to_str().unwrap(),
        ])
        .assert()
        .success();
    artifact
}

/// A project with the 1.0.0 release (skill + workflow) installed.
fn project_with_pack_release(artifact: &Path, home: &Path) -> TempDir {
    let project = TempDir::new().unwrap();
    tuff()
        .current_dir(project.path())
        .env("HOME", home)
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home)
        .args([
            "add",
            "pack",
            artifact.to_str().unwrap(),
            "--agent",
            "open-agents",
        ])
        .assert()
        .success();
    project
}

#[test]
fn update_pack_from_artifact_moves_every_member_forward() {
    let author = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let older = make_pack_release(author.path(), "1.0.0", "# v1\n", true, false);
    let newer = make_pack_release(author.path(), "1.1.0", "# v2\n", false, true);
    let project = project_with_pack_release(&older, home.path());
    assert!(
        project
            .path()
            .join(".agents/skills/tuff-capabilities/SKILL.md")
            .is_file(),
        "the workflow gives the 1.0.0 release something to index"
    );

    // Naming any member updates the whole pack.
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["update", "pack-workflow", "--pack", newer.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "updated pack com.acme/engineering 1.0.0 -> 1.1.0",
        ))
        .stdout(predicate::str::contains("added pack-notes 1.0.0"))
        .stdout(predicate::str::contains("updated pack-skill 1.1.0"))
        .stdout(predicate::str::contains("removed pack-workflow"));

    assert_eq!(
        fs::read_to_string(project.path().join(".agents/skills/pack-skill/SKILL.md")).unwrap(),
        "# v2\n"
    );
    assert!(
        project
            .path()
            .join(".agents/skills/pack-notes/SKILL.md")
            .is_file()
    );
    assert!(
        !project.path().join(".agents/workflows").exists(),
        "a member dropped by the new release is removed, directory included"
    );
    assert!(
        !project
            .path()
            .join(".agents/skills/tuff-capabilities")
            .exists(),
        "the derived index goes with the last workflow rather than lingering untracked"
    );

    let lock = fs::read_to_string(project.path().join("tuff.lock")).unwrap();
    assert!(!lock.contains("pack-workflow"));
    assert!(lock.contains("name = \"pack-notes\""));
    assert!(!lock.contains("version = \"1.0.0\"\ndigest"));
    assert_eq!(
        lock.matches("version = \"1.1.0\"\ndigest").count(),
        2,
        "both members carry the new release's provenance: {lock}"
    );

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("check")
        .assert()
        .success();

    // Applying the same release again is a no-op, not a reinstall.
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["update", "pack-skill", "--pack", newer.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "pack com.acme/engineering is already up to date (1.1.0)",
        ));
}

#[test]
fn update_pack_check_previews_the_release_without_changing_files() {
    let author = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let older = make_pack_release(author.path(), "1.0.0", "# v1\n", true, false);
    let newer = make_pack_release(author.path(), "1.1.0", "# v2\n", false, true);
    let project = project_with_pack_release(&older, home.path());
    let lock_before = fs::read_to_string(project.path().join("tuff.lock")).unwrap();

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "update",
            "pack-skill",
            "--check",
            "--pack",
            newer.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "pack com.acme/engineering can be updated 1.0.0 -> 1.1.0 for open-agents",
        ))
        .stdout(predicate::str::contains("add pack-notes 1.0.0"))
        .stdout(predicate::str::contains("update pack-skill 1.1.0"))
        .stdout(predicate::str::contains("remove pack-workflow"))
        .stdout(predicate::str::contains("would apply cleanly"));

    assert_eq!(
        fs::read_to_string(project.path().join("tuff.lock")).unwrap(),
        lock_before
    );
    assert_eq!(
        fs::read_to_string(project.path().join(".agents/skills/pack-skill/SKILL.md")).unwrap(),
        "# v1\n"
    );
    assert!(
        project
            .path()
            .join(".agents/workflows/pack-workflow/workflow.toml")
            .is_file()
    );
}

#[test]
fn update_pack_refuses_local_changes_unless_forced() {
    let author = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let older = make_pack_release(author.path(), "1.0.0", "# v1\n", true, false);
    let newer = make_pack_release(author.path(), "1.1.0", "# v2\n", false, true);
    let project = project_with_pack_release(&older, home.path());
    let skill = project.path().join(".agents/skills/pack-skill/SKILL.md");
    fs::write(&skill, "# v1, edited locally\n").unwrap();

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "update",
            "pack-skill",
            "--check",
            "--pack",
            newer.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "local changes in pack-skill (open-agents); the update would need --force",
        ));

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["update", "pack-skill", "--pack", newer.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "pack com.acme/engineering has local changes in pack-skill (open-agents)",
        ))
        .stderr(predicate::str::contains("--force"));
    assert_eq!(
        fs::read_to_string(&skill).unwrap(),
        "# v1, edited locally\n",
        "a refused update leaves the project untouched"
    );

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "update",
            "pack-skill",
            "--force",
            "--pack",
            newer.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert_eq!(fs::read_to_string(&skill).unwrap(), "# v2\n");
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("check")
        .assert()
        .success();
}

#[test]
fn update_pack_rejects_another_pack_a_narrower_agent_selection_and_non_pack_use() {
    let author = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let older = make_pack_release(author.path(), "1.0.0", "# v1\n", true, false);
    let newer = make_pack_release(author.path(), "1.1.0", "# v2\n", false, true);
    let project = project_with_pack_release(&older, home.path());

    // A different pack under the same member ids is not an update.
    let other = make_pack(author.path());
    let other_manifest = fs::read_to_string(other.join("tuff-pack.toml")).unwrap();
    fs::write(
        other.join("tuff-pack.toml"),
        other_manifest.replace("com.acme/engineering", "com.acme/other"),
    )
    .unwrap();
    let other_artifact = author.path().join("other.tuffpack");
    tuff()
        .current_dir(author.path())
        .args([
            "pack",
            "build",
            other.to_str().unwrap(),
            "--output",
            other_artifact.to_str().unwrap(),
        ])
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "update",
            "pack-skill",
            "--pack",
            other_artifact.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "refusing to update pack com.acme/engineering from an artifact for pack com.acme/other",
        ));

    // The pack moves for every agent it is installed for, or not at all.
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "update",
            "pack-skill",
            "--agent",
            "claude",
            "--pack",
            newer.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "a pack update applies to every agent the pack is installed for (open-agents)",
        ))
        .stderr(predicate::str::contains("hint: drop --agent"));

    // Without a registry on record and without --pack there is nothing to
    // resolve against, and the message says how to proceed.
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["update", "pack-skill"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "pack com.acme/engineering was installed without --reference",
        ))
        .stderr(predicate::str::contains("--pack <artifact>"));

    // --pack is meaningless for a capability that did not come from a pack.
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "update",
            "tuff-cli-guide",
            "--pack",
            newer.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--pack only applies to a capability installed from a pack; 'tuff-cli-guide' was not",
        ));

    let lock = fs::read_to_string(project.path().join("tuff.lock")).unwrap();
    assert!(
        lock.contains("version = \"1.0.0\"\ndigest"),
        "nothing above changed the install"
    );
}

#[test]
fn update_pack_replaces_shared_hook_and_mcp_registrations() {
    let author = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();

    // 1.0.0: an MCP-native tool and a hook, both registered in shared files.
    let pack = make_runtime_pack(author.path());
    let older = author.path().join("runtime-1.0.0.tuffpack");
    tuff()
        .current_dir(author.path())
        .args([
            "pack",
            "build",
            pack.to_str().unwrap(),
            "--output",
            older.to_str().unwrap(),
        ])
        .assert()
        .success();

    // 1.1.0: the tool only. The hook registration must leave with it.
    let manifest_path = pack.join("tuff-pack.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        manifest
            .replace("version = \"1.0.0\"", "version = \"1.1.0\"")
            .replace(
                "\n[[capabilities]]\npath = \"capabilities/hook-primitive\"\n",
                "\n",
            ),
    )
    .unwrap();
    let newer = author.path().join("runtime-1.1.0.tuffpack");
    tuff()
        .current_dir(author.path())
        .args([
            "pack",
            "build",
            pack.to_str().unwrap(),
            "--output",
            newer.to_str().unwrap(),
        ])
        .assert()
        .success();

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();
    fs::write(
        project.path().join(".agents/mcp.json"),
        r#"{"custom":{"preserved":true}}"#,
    )
    .unwrap();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "add",
            "pack",
            older.to_str().unwrap(),
            "--agent",
            "open-agents",
        ])
        .assert()
        .success();
    let hooks = fs::read_to_string(project.path().join(".agents/hook.json")).unwrap();
    assert!(hooks.contains("pack-hook"));

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["update", "pack-mcp", "--pack", newer.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed pack-hook"));

    let mcp: serde_json::Value =
        serde_json::from_slice(&fs::read(project.path().join(".agents/mcp.json")).unwrap())
            .unwrap();
    assert_eq!(
        mcp["custom"]["preserved"], true,
        "neighbouring config survives"
    );
    assert!(mcp["mcpServers"]["pack-mcp"].is_object());
    let hooks = fs::read_to_string(project.path().join(".agents/hook.json")).unwrap_or_default();
    assert!(
        !hooks.contains("pack-hook"),
        "the dropped hook's registration is gone: {hooks}"
    );
    assert!(!project.path().join(".agents/hooks/pack-hook").exists());
    let lock = fs::read_to_string(project.path().join("tuff.lock")).unwrap();
    assert!(!lock.contains("pack-hook"));
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("check")
        .assert()
        .success();
}

#[test]
fn add_pack_into_a_project_that_already_has_a_capability_index() {
    // Any installed tool, workflow, or MCP server gives the project a
    // tracked capability index. A pack install regenerates that index in
    // staging and must be allowed to replace the tracked copy; refusing it
    // as an "untracked file" made pack installs impossible for exactly the
    // projects most likely to want them.
    let author = TempDir::new().unwrap();
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let pack = make_pack(author.path());
    let artifact = author.path().join("pack.tuffpack");
    tuff()
        .current_dir(author.path())
        .args([
            "pack",
            "build",
            pack.to_str().unwrap(),
            "--output",
            artifact.to_str().unwrap(),
        ])
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();
    let tool = make_tool_primitive(project.path(), "existing-tool");
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["add", tool.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();
    let index = project
        .path()
        .join(".agents/skills/tuff-capabilities/SKILL.md");
    assert!(index.is_file(), "the tool install produced an index");
    assert!(
        fs::read_to_string(&index)
            .unwrap()
            .contains("existing-tool")
    );

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "add",
            "pack",
            artifact.to_str().unwrap(),
            "--agent",
            "open-agents",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed pack com.acme/engineering 1.2.0",
        ));

    let rendered = fs::read_to_string(&index).unwrap();
    assert!(rendered.contains("existing-tool"), "{rendered}");
    assert!(rendered.contains("pack-workflow"), "{rendered}");
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("check")
        .assert()
        .success();
}

// ── lockfile schema v2 (RFC-105) ─────────────────────────────────────

fn lockfile_v1_fixture() -> std::path::PathBuf {
    test_fixture("lockfile-v1").join("tuff.lock")
}

#[test]
fn lock_migrate_rewrites_a_version_1_lockfile_to_the_golden_version_2() {
    // The fixture was written by tuff 0.1.8 and covers every row shape:
    // local, git, catalog, pack with a registry, an adopted (imported)
    // capability, a hook with managed settings, an MCP-native tool, and
    // the generated index. The golden file is what migration must produce,
    // byte for byte, and it must be a fixed point of the writer.
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    fs::copy(lockfile_v1_fixture(), project.path().join("tuff.lock")).unwrap();
    let expected =
        fs::read_to_string(test_fixture("lockfile-v1").join("expected-v2.lock")).unwrap();

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["lock", "migrate"])
        .assert()
        .success()
        .stdout(predicate::str::contains("from schema version 1 to 2"));
    assert_eq!(
        fs::read_to_string(project.path().join("tuff.lock")).unwrap(),
        expected
    );

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["lock", "migrate"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already schema version 2"));
    assert_eq!(
        fs::read_to_string(project.path().join("tuff.lock")).unwrap(),
        expected
    );
}

#[test]
fn read_only_commands_leave_a_version_1_lockfile_alone_and_a_mutating_one_upgrades_it() {
    // `tuff check` in CI must never dirty the tree just by reading a v1
    // file; the first command that writes the lockfile is what upgrades it.
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let v1 = fs::read_to_string(lockfile_v1_fixture()).unwrap();
    fs::write(project.path().join("tuff.lock"), &v1).unwrap();
    fs::write(
        project.path().join("tuff.config.json"),
        r#"{"agents":["open-agents"],"defaultAgent":"open-agents"}"#,
    )
    .unwrap();

    for args in [
        vec!["list"],
        vec!["status"],
        vec!["check", "--ignore-failures"],
    ] {
        tuff()
            .current_dir(project.path())
            .env("HOME", home.path())
            .args(&args)
            .assert()
            .success();
        assert_eq!(
            fs::read_to_string(project.path().join("tuff.lock")).unwrap(),
            v1,
            "{args:?} rewrote a v1 lockfile"
        );
    }
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("git-skill"))
        .stdout(predicate::str::contains("pack-skill"));

    let skill = make_skill_primitive_dir(project.path(), "fresh-skill");
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["add", skill.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();
    let lock = fs::read_to_string(project.path().join("tuff.lock")).unwrap();
    assert!(
        lock.starts_with(
            "# Tuff lockfile. Each entry records one capability installation target.\nversion = 2\n"
        ),
        "{lock}"
    );
    assert!(lock.contains("name = \"fresh-skill\""));
    assert!(
        lock.contains("kind = \"pack\""),
        "existing rows survive the upgrade: {lock}"
    );
    assert!(!lock.contains("resolved_ref"));
}

#[test]
fn a_lockfile_from_a_newer_tuff_is_refused_by_version_not_by_shape() {
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    fs::write(
        project.path().join("tuff.lock"),
        "version = 3\n\n[[capabilities]]\nname = \"x\"\nfuture_field = true\n",
    )
    .unwrap();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["lock", "migrate"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported lockfile version: 3"))
        .stderr(predicate::str::contains("upgrade tuff"))
        .stderr(predicate::str::contains("future_field").not());
}

#[test]
fn a_project_add_never_writes_to_the_global_lockfile_even_with_xdg_state_home() {
    // Debt #12: the old lockfile-path helper treated the repository root as
    // a home directory and honoured XDG_STATE_HOME unconditionally, so on a
    // machine that had ever used --global, a project-scoped add could land
    // in the global lockfile. The scope is explicit now.
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let state = home.path().join("state");
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .env("XDG_STATE_HOME", &state)
        .args(["init", "--global"])
        .assert()
        .success();
    let global_lock = state.join("tuff").join("tuff.lock");
    assert!(
        global_lock.is_file(),
        "global init wrote {}",
        global_lock.display()
    );
    let global_before = fs::read_to_string(&global_lock).unwrap();

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .env("XDG_STATE_HOME", &state)
        .arg("init")
        .assert()
        .success();
    let skill = make_skill_primitive_dir(project.path(), "project-only");
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .env("XDG_STATE_HOME", &state)
        .args(["add", skill.to_str().unwrap(), "--agent", "open-agents"])
        .assert()
        .success();

    let project_lock = fs::read_to_string(project.path().join("tuff.lock")).unwrap();
    assert!(
        project_lock.contains("name = \"project-only\""),
        "{project_lock}"
    );
    assert_eq!(
        fs::read_to_string(&global_lock).unwrap(),
        global_before,
        "the global lockfile must be untouched by a project-scoped add"
    );
}

/// A minimal manifest-backed skill directory outside any harness layout.
fn make_skill_primitive_dir(root: &Path, id: &str) -> std::path::PathBuf {
    let dir = root.join("sources").join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("tuff.toml"),
        format!(
            "id = \"{id}\"\nversion = \"1.0.0\"\ntype = \"skill\"\ndescription = \"A local skill.\"\nfiles = [\"SKILL.md\"]\n"
        ),
    )
    .unwrap();
    fs::write(dir.join("SKILL.md"), format!("# {id}\n")).unwrap();
    dir
}

// ── typed errors (RFC-105 D6) ────────────────────────────────────────

#[test]
fn exit_codes_distinguish_usage_from_failure() {
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();

    // Success.
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("list")
        .assert()
        .code(0);

    // A capability that is not installed is an ordinary failure.
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["delete", "no-such-capability", "--scope", "project"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("is not installed"))
        .stderr(predicate::str::contains("hint: run 'tuff list'"));

    // A scope that is not a scope is the caller's mistake, not a failure
    // of the operation, and scripts branch on that difference.
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["delete", "anything", "--scope", "sideways"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("invalid scope 'sideways'"))
        .stderr(predicate::str::contains(
            "hint: scope is 'project' or 'global'",
        ));
}

#[test]
fn a_json_invocation_reports_failure_as_json_on_stderr() {
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();

    // A missing lockfile: the caller asked for machine-readable output, so
    // the failure must be machine-readable too, not prose.
    let assert = tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["mcp", "doctor", "--json"])
        .assert()
        .code(1);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let envelope: serde_json::Value = serde_json::from_str(stderr.trim())
        .unwrap_or_else(|error| panic!("stderr was not one JSON line ({error}): {stderr}"));
    assert_eq!(envelope["error"]["kind"], "not_found");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("tuff.lock"),
        "{envelope}"
    );
    assert_eq!(envelope["error"]["hint"], "run 'tuff init' first");

    // A corrupt lockfile is a different kind, and carries no hint.
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();
    fs::write(
        project.path().join("tuff.lock"),
        "version = 2\n[[capabilities]\nname = \"broken\"\n",
    )
    .unwrap();
    let assert = tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["check", "--json"])
        .assert()
        .code(1);
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    let envelope: serde_json::Value = serde_json::from_str(stderr.trim()).unwrap();
    assert_eq!(envelope["error"]["kind"], "corrupt");
    assert!(envelope["error"].get("hint").is_none(), "{envelope}");
}

#[test]
fn a_corrupt_lockfile_is_reported_everywhere_it_used_to_read_as_empty() {
    // list, status, outdated, and check each walked both lockfiles with a
    // silent `if let Ok(..)`. A syntactically broken tuff.lock therefore
    // rendered as "no capabilities installed", the most misleading possible
    // answer for a tool whose job is knowing what is installed.
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();
    fs::write(
        project.path().join("tuff.lock"),
        "version = 2\n[[capabilities]\nname = \"broken\"\n",
    )
    .unwrap();

    for args in [
        vec!["list"],
        vec!["status"],
        vec!["outdated"],
        vec!["check"],
    ] {
        tuff()
            .current_dir(project.path())
            .env("HOME", home.path())
            .args(&args)
            .assert()
            .code(1)
            .stderr(predicate::str::contains("not a valid lockfile"))
            .stdout(predicate::str::contains("no capabilities installed").not());
    }
}

#[test]
fn a_missing_global_lockfile_is_not_an_error() {
    // The global lockfile legitimately does not exist until someone uses
    // --global. Distinguishing that from a corrupt file is the whole point
    // of the change above, so it needs its own test.
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();
    for args in [
        vec!["list"],
        vec!["status"],
        vec!["outdated"],
        vec!["check"],
    ] {
        tuff()
            .current_dir(project.path())
            .env("HOME", home.path())
            .args(&args)
            .assert()
            .success();
    }
}

// ── MCP over HTTP (stub-backed, no network) ──────────────────────────

/// How the stub answers, so one server covers every branch the probe has.
#[derive(Clone, Copy, PartialEq)]
enum StubMode {
    /// `application/json` responses, the simpler of the two legal shapes.
    Json,
    /// `text/event-stream` responses, with an unrelated notification ahead
    /// of the answer so the probe has to skip it.
    EventStream,
    /// Refuses everything, as a server with a bad token would.
    Unauthorized,
}

/// A stub Streamable HTTP MCP server.
///
/// Real remote servers need credentials nobody should put in a test, and a
/// test that dialled one would depend on someone else's uptime. This speaks
/// enough of the transport to exercise the probe: both response shapes, the
/// session id, and a 401.
struct StubMcpServer {
    port: u16,
    seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    _thread: std::thread::JoinHandle<()>,
}

impl StubMcpServer {
    fn start(mode: StubMode) -> Self {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = std::sync::Arc::clone(&seen);

        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut raw = Vec::new();
                let mut buffer = [0u8; 1024];
                // Read headers, then exactly as much body as Content-Length
                // promises: the client keeps the socket open for the reply,
                // so reading to EOF would hang.
                let request = loop {
                    let Ok(read) = stream.read(&mut buffer) else {
                        break None;
                    };
                    if read == 0 {
                        break None;
                    }
                    raw.extend_from_slice(&buffer[..read]);
                    let text = String::from_utf8_lossy(&raw).to_string();
                    let Some(head_end) = text.find("\r\n\r\n") else {
                        continue;
                    };
                    let head = &text[..head_end];
                    let length: usize = head
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse().ok())?
                        })
                        .unwrap_or(0);
                    if text.len() >= head_end + 4 + length {
                        break Some(text);
                    }
                };
                let Some(request) = request else { continue };
                recorded.lock().unwrap().push(request.clone());

                let response = stub_response(mode, &request);
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });

        Self {
            port,
            seen,
            _thread: thread,
        }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}/mcp", self.port)
    }

    fn requests(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }
}

fn stub_response(mode: StubMode, request: &str) -> String {
    if mode == StubMode::Unauthorized {
        return http_response(
            "401 Unauthorized",
            "application/json",
            "{\"error\":\"invalid token\"}",
            &["WWW-Authenticate: Bearer realm=\"stub\""],
        );
    }

    // A notification has no id and expects no body, only an acknowledgement.
    if request.contains("notifications/initialized") {
        return "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_string();
    }

    let (payload, extra): (String, Vec<String>) = if request.contains("\"initialize\"") {
        (
            r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{},"serverInfo":{"name":"stub","version":"1.0.0"}}}"#
                .to_string(),
            vec!["Mcp-Session-Id: stub-session-42".to_string()],
        )
    } else {
        (
            r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"search"},{"name":"create_page"}]}}"#
                .to_string(),
            Vec::new(),
        )
    };

    let extra: Vec<&str> = extra.iter().map(String::as_str).collect();
    match mode {
        StubMode::Json => http_response("200 OK", "application/json", &payload, &extra),
        StubMode::EventStream => {
            // A notification the probe has to skip, then the answer.
            let body = format!(
                "event: message\ndata: {{\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}}\n\nevent: message\ndata: {payload}\n\n"
            );
            http_response("200 OK", "text/event-stream", &body, &extra)
        }
        StubMode::Unauthorized => unreachable!("handled above"),
    }
}

fn http_response(status: &str, content_type: &str, body: &str, extra: &[&str]) -> String {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for header in extra {
        head.push_str(header);
        head.push_str("\r\n");
    }
    format!("{head}\r\n{body}")
}

/// Install one remote server declaring a `Bearer` auth header, pointed at
/// `url`, and return the project directory.
fn project_with_remote_server(url: &str) -> TempDir {
    let temp = TempDir::new().unwrap();
    let primitive = temp.path().join("remote-primitive");
    fs::create_dir_all(&primitive).unwrap();
    fs::write(
        primitive.join("tuff.toml"),
        format!(
            r#"id = "remote-stub"
version = "1.0.0"
type = "mcp-server"
description = "A stubbed remote MCP server."

[server]
transport = "http"
url = "{url}"

[server.headers]
Authorization = {{ from_env = "STUB_HTTP_TOKEN", format = "Bearer {{}}" }}
"#
        ),
    )
    .unwrap();

    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .args([
            "add",
            "mcp",
            primitive.to_str().unwrap(),
            "-a",
            "open-agents",
        ])
        .assert()
        .success();
    temp
}

#[test]
fn mcp_doctor_probes_an_http_server_answering_with_json() {
    let stub = StubMcpServer::start(StubMode::Json);
    let project = project_with_remote_server(&stub.url());

    tuff()
        .current_dir(project.path())
        .env("STUB_HTTP_TOKEN", "secret-token")
        .args(["mcp", "doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("remote-stub"))
        .stdout(predicate::str::contains("http"))
        .stdout(predicate::str::contains("ok"))
        .stdout(predicate::str::contains("2 tool(s)"));

    let requests = stub.requests();
    assert_eq!(requests.len(), 3, "initialize, notification, tools/list");
    // The declared header reaches the server with the real value assembled
    // through `format`, never the `${VAR}` reference a config file carries.
    // Header names arrive lowercased, so match them that way throughout.
    let requests: Vec<String> = requests.iter().map(|r| r.to_lowercase()).collect();
    assert!(
        requests
            .iter()
            .all(|request| request.contains("authorization: bearer secret-token")),
        "{requests:?}"
    );
    // The session id the server issued on initialize is echoed afterwards,
    // and cannot have been sent before the server issued it.
    assert!(!requests[0].contains("mcp-session-id"), "{}", requests[0]);
    assert!(
        requests[2].contains("mcp-session-id: stub-session-42"),
        "{}",
        requests[2]
    );
    // As is the version the server negotiated.
    assert!(
        requests[2].contains("mcp-protocol-version: 2024-11-05"),
        "{}",
        requests[2]
    );
}

#[test]
fn mcp_doctor_probes_an_http_server_answering_with_an_event_stream() {
    let stub = StubMcpServer::start(StubMode::EventStream);
    let project = project_with_remote_server(&stub.url());

    tuff()
        .current_dir(project.path())
        .env("STUB_HTTP_TOKEN", "secret-token")
        .args(["mcp", "doctor", "--json"])
        .assert()
        .success();

    let output = tuff()
        .current_dir(project.path())
        .env("STUB_HTTP_TOKEN", "secret-token")
        .args(["mcp", "doctor", "--json"])
        .output()
        .unwrap();
    let rows: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows[0]["status"], "ok");
    assert_eq!(rows[0]["transport"], "http");
    assert_eq!(
        rows[0]["tools"],
        serde_json::json!(["search", "create_page"])
    );
}

#[test]
fn mcp_doctor_reports_a_refused_token_as_unauthorized() {
    let stub = StubMcpServer::start(StubMode::Unauthorized);
    let project = project_with_remote_server(&stub.url());

    tuff()
        .current_dir(project.path())
        .env("STUB_HTTP_TOKEN", "wrong-token")
        .args(["mcp", "doctor"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("unauthorized"))
        // The challenge names what the server wanted, which is the useful
        // part for someone whose token was refused.
        .stdout(predicate::str::contains("Bearer realm"));
}

#[test]
fn mcp_doctor_reports_a_host_that_is_not_listening_as_unreachable() {
    // Bind and drop, so the port is real, free, and refusing connections.
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let project = project_with_remote_server(&format!("http://127.0.0.1:{port}/mcp"));

    tuff()
        .current_dir(project.path())
        .env("STUB_HTTP_TOKEN", "secret-token")
        .args(["mcp", "doctor"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("unreachable"));
}

#[test]
fn mcp_doctor_reports_an_unexported_header_variable_without_making_a_request() {
    let stub = StubMcpServer::start(StubMode::Json);
    let project = project_with_remote_server(&stub.url());

    tuff()
        .current_dir(project.path())
        .env_remove("STUB_HTTP_TOKEN")
        .args(["mcp", "doctor"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("missing env"))
        .stdout(predicate::str::contains("export STUB_HTTP_TOKEN"));

    // The point of checking first: no credential-less request is made, so
    // nothing about this project reaches the server.
    assert!(stub.requests().is_empty());
}

// ── MCP registry (stub-backed, no network) ───────────────────────────

/// A one-shot HTTP server returning a canned registry response.
///
/// The registry paths are worth testing end to end, but a test that calls
/// the real registry would depend on the network and on whatever third
/// parties happen to have published that day. This serves bytes we control.
struct StubRegistry {
    port: u16,
    _thread: std::thread::JoinHandle<()>,
}

impl StubRegistry {
    fn start(body: &str) -> Self {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let body = body.to_string();
        let thread = std::thread::spawn(move || {
            // Serve every request the test makes, then fall out when the
            // listener is dropped at the end of the test.
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut buffer = [0u8; 2048];
                let _ = stream.read(&mut buffer);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        Self {
            port,
            _thread: thread,
        }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

fn stub_server_json(name: &str) -> String {
    format!(
        r#"{{"servers":[{{"server":{{"name":"{name}","description":"A stubbed server.","version":"2.1.0","packages":[{{"registryType":"npm","identifier":"stub-mcp","version":"2.1.0","transport":{{"type":"stdio"}},"environmentVariables":[{{"name":"STUB_TOKEN","isRequired":true}}]}}]}}}}]}}"#
    )
}

/// A remote registry entry: one required header the publisher documents as
/// `Bearer {vendor_api_key}`, and one optional header Tuff leaves out.
fn stub_remote_json(name: &str) -> String {
    format!(
        r#"{{"servers":[{{"server":{{"name":"{name}","description":"A stubbed remote server.","version":"3.0.0","remotes":[{{"type":"sse","url":"https://legacy.example.test/sse"}},{{"type":"streamable-http","url":"https://mcp.example.test/mcp","headers":[{{"name":"Authorization","isRequired":true,"isSecret":true,"value":"Bearer {{vendor_api_key}}"}},{{"name":"X-Request-Id","isRequired":false}}]}}]}}}}]}}"#
    )
}

#[test]
fn add_mcp_installs_a_remote_registry_entry_that_authenticates_with_a_header() {
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let registry = StubRegistry::start(&stub_remote_json("com.acme/remote-mcp"));
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "add",
            "mcp",
            "com.acme/remote-mcp",
            "--agent",
            "open-agents",
            "--yes",
            "--registry",
            &registry.url(),
        ])
        .assert()
        .success()
        // The variable the publisher named, not one Tuff invented.
        .stderr(predicate::str::contains("export VENDOR_API_KEY"))
        // What was left out is said out loud rather than dropped silently.
        .stderr(predicate::str::contains("optional header X-Request-Id"));

    let mcp: serde_json::Value =
        serde_json::from_slice(&fs::read(project.path().join(".agents/mcp.json")).unwrap())
            .unwrap();
    let entry = &mcp["mcpServers"]["remote-mcp"];
    // `streamable-http` wins over the `sse` remote listed first.
    assert_eq!(entry["url"], "https://mcp.example.test/mcp");
    assert_eq!(entry["type"], "http");
    assert_eq!(
        entry["headers"]["Authorization"],
        "Bearer ${VENDOR_API_KEY}"
    );
    assert!(entry["headers"]["X-Request-Id"].is_null());

    // The record keeps the documented template, and no secret.
    let record = fs::read_to_string(
        project
            .path()
            .join(".agents/mcp-servers/remote-mcp/server.toml"),
    )
    .unwrap();
    assert!(record.contains("from_env = \"VENDOR_API_KEY\""), "{record}");
    assert!(record.contains("format = \"Bearer {}\""), "{record}");

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("check")
        .assert()
        .success();
}

#[test]
fn mcp_search_marks_a_header_authenticated_remote_as_installable() {
    let temp = TempDir::new().unwrap();
    let registry = StubRegistry::start(&stub_remote_json("com.acme/remote-mcp"));

    let output = tuff()
        .current_dir(temp.path())
        .args([
            "mcp",
            "search",
            "remote",
            "--json",
            "--registry",
            &registry.url(),
        ])
        .output()
        .unwrap();
    let hits: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(hits[0]["name"], "com.acme/remote-mcp");
    assert_eq!(hits[0]["id"], "remote-mcp");
    assert_eq!(hits[0]["installable"], true, "{hits}");
    assert!(hits[0]["detail"].is_null(), "{hits}");

    // The table is where someone actually reads this: the INSTALL column
    // used to say `unsupported` for every header-authenticated entry.
    tuff()
        .current_dir(temp.path())
        .args(["mcp", "search", "remote", "--registry", &registry.url()])
        .assert()
        .success()
        .stdout(predicate::str::contains("com.acme/remote-mcp"))
        .stdout(predicate::str::contains("http"))
        .stdout(predicate::str::contains("unsupported").not());
}

#[test]
fn add_mcp_installs_a_server_resolved_from_the_registry() {
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let registry = StubRegistry::start(&stub_server_json("io.github.acme/stub-mcp"));
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "add",
            "mcp",
            "io.github.acme/stub-mcp",
            "--agent",
            "open-agents",
            "--yes",
            "--registry",
            &registry.url(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed stub-mcp from http://127.0.0.1",
        ));

    // The launch command is assembled from the package, not copied.
    let mcp: serde_json::Value =
        serde_json::from_slice(&fs::read(project.path().join(".agents/mcp.json")).unwrap())
            .unwrap();
    assert_eq!(mcp["mcpServers"]["stub-mcp"]["command"], "npx");
    assert_eq!(
        mcp["mcpServers"]["stub-mcp"]["args"],
        serde_json::json!(["-y", "stub-mcp@2.1.0"])
    );
    // A required variable is a reference the harness resolves, never a value.
    assert_eq!(
        mcp["mcpServers"]["stub-mcp"]["env"]["STUB_TOKEN"],
        "${STUB_TOKEN}"
    );

    // The lockfile records which registry it came from, so update and
    // outdated know to ask that registry rather than the built-in catalog.
    let lock = fs::read_to_string(project.path().join("tuff.lock")).unwrap();
    assert!(lock.contains("kind = \"catalog\""), "{lock}");
    assert!(lock.contains("id = \"io.github.acme/stub-mcp\""), "{lock}");
    assert!(lock.contains("registry = \"http://127.0.0.1"), "{lock}");

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("check")
        .assert()
        .success();
}

#[test]
fn mcp_search_reports_what_each_result_would_install() {
    let registry = StubRegistry::start(&stub_server_json("io.github.acme/stub-mcp"));
    tuff()
        .args(["mcp", "search", "stub", "--registry", &registry.url()])
        .assert()
        .success()
        .stdout(predicate::str::contains("io.github.acme/stub-mcp"))
        .stdout(predicate::str::contains("2.1.0"))
        .stdout(predicate::str::contains("npx"))
        .stdout(predicate::str::contains("tuff add mcp <NAME>"));

    let assert = tuff()
        .args([
            "mcp",
            "search",
            "stub",
            "--json",
            "--registry",
            &registry.url(),
        ])
        .assert()
        .success();
    let rows: serde_json::Value =
        serde_json::from_slice(&assert.get_output().stdout.clone()).unwrap();
    assert_eq!(rows[0]["name"], "io.github.acme/stub-mcp");
    assert_eq!(rows[0]["id"], "stub-mcp");
    assert_eq!(rows[0]["installable"], true);
}

#[test]
fn an_exact_name_is_required_so_a_search_hit_never_installs_by_surprise() {
    // The registry has no exact-name endpoint, so `add mcp <name>` searches
    // and matches the name itself. Installing the first hit for a partial
    // name would let a typo pull in somebody else's fork.
    let project = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let registry = StubRegistry::start(&stub_server_json("io.github.someone-else/stub-mcp"));
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "add",
            "mcp",
            "stub-mcp",
            "--agent",
            "open-agents",
            "--yes",
            "--registry",
            &registry.url(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "is not a path, a git URL, a built-in catalog id, or a server in the MCP registry",
        ));
    assert!(!project.path().join(".agents/mcp-servers/stub-mcp").exists());
}
