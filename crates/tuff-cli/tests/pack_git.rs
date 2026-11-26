//! Packs installed from a git repository (RFC-110 milestone 1): install at
//! the newest release, follow tags and branches, preview updates as JSON,
//! list and remove packs as one unit.

use std::{fs, path::Path, process::Command};

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

fn tuff() -> Command {
    Command::cargo_bin("tuff").unwrap()
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.test"])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn head(dir: &Path) -> String {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

/// Write a pack with one skill and one tool into `folder` at `version`.
fn write_pack(folder: &Path, version: &str, skill_text: &str) {
    let skill = folder.join("capabilities/pack-skill");
    let tool = folder.join("capabilities/pack-tool");
    fs::create_dir_all(&skill).unwrap();
    fs::create_dir_all(&tool).unwrap();
    fs::write(
        folder.join("tuff-pack.toml"),
        format!(
            r#"schema = 1
name = "com.acme/engineering"
version = "{version}"
description = "A pack installed from git."

[build]
targets = ["open-agents", "claude"]

[[capabilities]]
path = "capabilities/pack-skill"

[[capabilities]]
path = "capabilities/pack-tool"
"#
        ),
    )
    .unwrap();
    fs::write(
        skill.join("tuff.toml"),
        format!(
            "id = \"pack-skill\"\nversion = \"{version}\"\ntype = \"skill\"\ndescription = \"A skill.\"\nfiles = [\"SKILL.md\"]\n"
        ),
    )
    .unwrap();
    fs::write(skill.join("SKILL.md"), skill_text).unwrap();
    fs::write(
        tool.join("tuff.toml"),
        r#"id = "pack-tool"
version = "1.0.0"
type = "tool"
description = "A tool."
files = ["run.sh"]

[parameters]
type = "object"

[parameters.properties.target]
type = "string"

[implementation]
language = "bash"
entrypoint = "run.sh"
"#,
    )
    .unwrap();
    fs::write(tool.join("run.sh"), "#!/bin/bash\necho packed\n").unwrap();
}

/// A repository with the pack at its root, committed on `main`.
fn pack_repo(root: &Path, version: &str, skill_text: &str) -> std::path::PathBuf {
    let repo = root.join("engineering-pack");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet", "--initial-branch", "main"]);
    write_pack(&repo, version, skill_text);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "release"]);
    repo
}

fn release(repo: &Path, version: &str, skill_text: &str) {
    write_pack(repo, version, skill_text);
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "--quiet", "-m", version]);
    git(repo, &["tag", &format!("v{version}")]);
}

fn url(repo: &Path) -> String {
    format!("file://{}", repo.display())
}

fn project(home: &Path) -> TempDir {
    let project = TempDir::new().unwrap();
    tuff()
        .current_dir(project.path())
        .env("HOME", home)
        .arg("init")
        .assert()
        .success();
    project
}

fn lockfile(project: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(project.join("tuff.lock")).unwrap()).unwrap()
}

fn json_stdout(command: &mut Command) -> serde_json::Value {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn a_pack_installs_from_git_at_its_newest_release_and_updates_to_the_next_tag() {
    let author = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let repo = pack_repo(author.path(), "1.0.0", "# v1\n");
    git(&repo, &["tag", "v1.0.0"]);
    release(&repo, "1.1.0", "# v1.1\n");
    let v1_1 = head(&repo);
    // A commit after the newest tag is not a release.
    write_pack(&repo, "1.2.0", "# unreleased\n");
    git(&repo, &["commit", "--quiet", "-am", "work in progress"]);

    let project = project(home.path());
    let installed = json_stdout(
        tuff()
            .current_dir(project.path())
            .env("HOME", home.path())
            .args(["add", "pack", &url(&repo), "-a", "open-agents", "--json"]),
    );
    assert_eq!(installed["schema"], 1);
    assert_eq!(installed["name"], "com.acme/engineering");
    assert_eq!(installed["version"], "1.1.0");
    assert_eq!(installed["source"]["kind"], "git");
    assert_eq!(installed["source"]["tag"], "v1.1.0");
    assert_eq!(installed["source"]["ref"], v1_1);
    assert_eq!(installed["members"].as_array().unwrap().len(), 2);
    assert_eq!(
        fs::read_to_string(project.path().join(".agents/skills/pack-skill/SKILL.md")).unwrap(),
        "# v1.1\n"
    );

    let lock = lockfile(project.path());
    assert_eq!(lock["version"], 4);
    let pack = &lock["packs"]["com.acme/engineering"];
    assert_eq!(pack["source"]["kind"], "git");
    assert_eq!(pack["source"]["ref"], v1_1);
    assert_eq!(
        pack["members"],
        serde_json::json!(["pack-skill", "pack-tool"])
    );

    let listing = json_stdout(
        tuff()
            .current_dir(project.path())
            .env("HOME", home.path())
            .args(["pack", "list", "--json"]),
    );
    assert_eq!(listing["packs"][0]["name"], "com.acme/engineering");
    assert_eq!(listing["packs"][0]["source"]["tag"], "v1.1.0");
    assert_eq!(
        listing["packs"][0]["members"][0]["targets"][0],
        "open-agents"
    );

    let outdated = json_stdout(
        tuff()
            .current_dir(project.path())
            .env("HOME", home.path())
            .args(["outdated", "--json"]),
    );
    let row = outdated
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "pack-skill")
        .unwrap()
        .clone();
    assert_eq!(row["status"], "up to date");

    release(&repo, "1.3.0", "# v1.3\n");
    let v1_3 = head(&repo);
    let outdated = json_stdout(
        tuff()
            .current_dir(project.path())
            .env("HOME", home.path())
            .args(["outdated", "--json"]),
    );
    let row = outdated
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "pack-skill")
        .unwrap()
        .clone();
    assert_eq!(row["status"], "outdated");
    assert_eq!(row["latest"], "1.3.0");

    let preview = json_stdout(
        tuff()
            .current_dir(project.path())
            .env("HOME", home.path())
            .args(["update", "pack-tool", "--check", "--json"]),
    );
    assert_eq!(preview["schema"], 1);
    assert_eq!(preview["status"], "update");
    assert_eq!(preview["from"]["version"], "1.1.0");
    assert_eq!(preview["to"]["version"], "1.3.0");
    assert_eq!(preview["to"]["source"]["tag"], "v1.3.0");
    assert_eq!(preview["blockedBy"], serde_json::json!([]));
    let members = preview["members"].as_array().unwrap();
    let skill = members.iter().find(|m| m["id"] == "pack-skill").unwrap();
    assert_eq!(skill["change"], "updated");
    assert_eq!(skill["fromVersion"], "1.1.0");
    assert_eq!(skill["toVersion"], "1.3.0");
    let file = &skill["files"][0];
    assert_eq!(file["path"], ".agents/skills/pack-skill/SKILL.md");
    assert_eq!(file["status"], "modified");
    assert!(
        file["patch"].as_str().unwrap().contains("+# v1.3"),
        "{file}"
    );
    let tool = members.iter().find(|m| m["id"] == "pack-tool").unwrap();
    assert_eq!(tool["change"], "unchanged");
    // A preview changes nothing.
    assert_eq!(
        fs::read_to_string(project.path().join(".agents/skills/pack-skill/SKILL.md")).unwrap(),
        "# v1.1\n"
    );

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["update", "pack-skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "updated pack com.acme/engineering 1.1.0 -> 1.3.0",
        ))
        .stdout(predicate::str::contains("tag v1.3.0"));
    assert_eq!(
        fs::read_to_string(project.path().join(".agents/skills/pack-skill/SKILL.md")).unwrap(),
        "# v1.3\n"
    );
    let lock = lockfile(project.path());
    assert_eq!(lock["packs"]["com.acme/engineering"]["source"]["ref"], v1_3);
    assert_eq!(lock["packs"]["com.acme/engineering"]["version"], "1.3.0");

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["update", "pack-skill"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already up to date"));
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("check")
        .assert()
        .success();
}

#[test]
fn a_tag_whose_manifest_declares_another_version_is_refused() {
    let author = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let repo = pack_repo(author.path(), "1.0.0", "# v1\n");
    git(&repo, &["tag", "v2.0.0"]);
    let project = project(home.path());
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["add", "pack", &url(&repo), "-a", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "tag v2.0.0 names version 2.0.0, but the manifest at that tag declares version 1.0.0",
        ));
    assert!(!project.path().join(".agents/skills/pack-skill").exists());
}

#[test]
fn a_repository_without_release_tags_installs_its_branch_and_follows_it() {
    let author = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let repo = pack_repo(author.path(), "1.0.0", "# first\n");
    let project = project(home.path());
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["add", "pack", &url(&repo), "-a", "open-agents"])
        .assert()
        .success()
        .stdout(predicate::str::contains("branch main at"))
        .stdout(predicate::str::contains("publishes no release tags"));
    assert_eq!(
        lockfile(project.path())["packs"]["com.acme/engineering"]["source"]["branch"],
        "main"
    );

    write_pack(&repo, "1.0.0", "# second\n");
    git(&repo, &["commit", "--quiet", "-am", "second"]);
    let second = head(&repo);
    let outdated = json_stdout(
        tuff()
            .current_dir(project.path())
            .env("HOME", home.path())
            .args(["outdated", "--json"]),
    );
    let row = outdated
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "pack-skill")
        .unwrap()
        .clone();
    assert_eq!(row["status"], "outdated");
    assert_eq!(row["latest"], &second[..7]);

    // Same version, new commit: a git pack updates by commit.
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["update", "pack-skill"])
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(project.path().join(".agents/skills/pack-skill/SKILL.md")).unwrap(),
        "# second\n"
    );
    assert_eq!(
        lockfile(project.path())["packs"]["com.acme/engineering"]["source"]["ref"],
        second
    );
}

#[test]
fn a_repointed_tag_is_reported_and_refused_without_force() {
    let author = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let repo = pack_repo(author.path(), "1.0.0", "# v1\n");
    git(&repo, &["tag", "v1.0.0"]);
    let project = project(home.path());
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["add", "pack", &url(&repo), "-a", "open-agents"])
        .assert()
        .success();

    write_pack(&repo, "1.0.0", "# moved\n");
    git(&repo, &["commit", "--quiet", "-am", "moved"]);
    git(&repo, &["tag", "-f", "v1.0.0"]);

    let outdated = json_stdout(
        tuff()
            .current_dir(project.path())
            .env("HOME", home.path())
            .args(["outdated", "--json"]),
    );
    assert!(
        outdated
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == "pack-skill" && row["status"] == "repointed"),
        "{outdated}"
    );
    let preview = json_stdout(
        tuff()
            .current_dir(project.path())
            .env("HOME", home.path())
            .args(["update", "pack-skill", "--check", "--json"]),
    );
    assert_eq!(preview["status"], "repointed");
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["update", "pack-skill"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("tag v1.0.0 now names commit"));
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["update", "pack-skill", "--force"])
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(project.path().join(".agents/skills/pack-skill/SKILL.md")).unwrap(),
        "# moved\n"
    );
}

#[test]
fn a_pack_in_a_subfolder_is_found_with_path_and_a_missing_manifest_says_where_it_looked() {
    let author = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let repo = author.path().join("monorepo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet", "--initial-branch", "main"]);
    write_pack(&repo.join("packs/engineering"), "1.0.0", "# sub\n");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "pack"]);
    git(&repo, &["tag", "engineering/v1.0.0"]);
    let project = project(home.path());

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["add", "pack", &url(&repo), "-a", "open-agents"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "no tuff-pack.toml or Claude Code plugin at the repository root",
        ))
        .stderr(predicate::str::contains("--path"));

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "add",
            "pack",
            &url(&repo),
            "--path",
            "packs/engineering",
            "-a",
            "open-agents",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("tag engineering/v1.0.0"));
    let source = &lockfile(project.path())["packs"]["com.acme/engineering"]["source"];
    assert_eq!(source["path"], "packs/engineering");
    assert_eq!(source["tag"], "engineering/v1.0.0");
}

#[test]
fn a_harness_the_pack_is_not_built_for_is_refused_before_anything_is_written() {
    let author = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let repo = pack_repo(author.path(), "1.0.0", "# v1\n");
    let project = project(home.path());
    let before = fs::read_to_string(project.path().join("tuff.lock")).unwrap();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["add", "pack", &url(&repo), "-a", "cursor"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "pack com.acme/engineering is built for claude, open-agents, not for cursor",
        ));
    assert_eq!(
        fs::read_to_string(project.path().join("tuff.lock")).unwrap(),
        before
    );
}

#[test]
fn a_pack_is_removed_as_one_unit_and_local_edits_need_force() {
    let author = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let repo = pack_repo(author.path(), "1.0.0", "# v1\n");
    let project = project(home.path());
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "add",
            "pack",
            &url(&repo),
            "-a",
            "open-agents",
            "-a",
            "claude",
        ])
        .assert()
        .success();
    let skill = project.path().join(".claude/skills/pack-skill/SKILL.md");
    fs::write(&skill, "# edited\n").unwrap();

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["pack", "remove", "com.acme/engineering"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "has local changes in pack-skill (claude)",
        ));
    assert!(skill.is_file());

    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["pack", "remove", "com.acme/engineering", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "removed pack com.acme/engineering 1.0.0",
        ));
    assert!(!skill.exists());
    assert!(!project.path().join(".agents/skills/pack-skill").exists());
    assert!(!project.path().join(".agents/tools/pack-tool").exists());
    let lock = lockfile(project.path());
    assert!(lock.get("packs").is_none(), "{lock}");
    assert!(
        !lock["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["name"] == "pack-skill" || row["name"] == "pack-tool"),
        "{lock}"
    );
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["pack", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no packs installed"));
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("check")
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["pack", "remove", "com.acme/engineering"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not installed"));
}

#[test]
fn ref_and_path_are_refused_for_an_artifact_and_reference_for_git() {
    let home = TempDir::new().unwrap();
    let project = project(home.path());
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["add", "pack", "missing.tuffpack", "--ref", "v1.0.0"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "apply to a pack installed from a git address",
        ));
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "add",
            "pack",
            "https://example.com/pack.git",
            "--reference",
            "ghcr.io/acme/pack:1.0.0",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "a pack from git records its repository itself",
        ));
}
