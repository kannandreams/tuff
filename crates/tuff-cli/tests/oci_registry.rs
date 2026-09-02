use std::{fs, path::Path, process::Command};

use assert_cmd::prelude::*;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

fn tuff() -> Command {
    Command::cargo_bin("tuff").unwrap()
}

fn make_pack(root: &Path, version: &str, content: &str, artifact: &Path) {
    let pack = root.join(format!("pack-{version}"));
    let skill = pack.join("capabilities/registry-skill");
    fs::create_dir_all(&skill).unwrap();
    fs::write(
        pack.join("tuff-pack.toml"),
        format!(
            r#"schema = 1
name = "com.acme/registry-test"
version = "{version}"
description = "OCI registry integration test pack."

[build]
targets = ["open-agents"]

[[capabilities]]
path = "capabilities/registry-skill"
"#
        ),
    )
    .unwrap();
    fs::write(
        skill.join("tuff.toml"),
        format!(
            r#"id = "registry-skill"
version = "{version}"
type = "skill"
description = "A skill distributed through OCI."
files = ["SKILL.md"]
"#
        ),
    )
    .unwrap();
    fs::write(skill.join("SKILL.md"), content).unwrap();

    tuff()
        .args([
            "pack",
            "build",
            pack.to_str().unwrap(),
            "--output",
            artifact.to_str().unwrap(),
        ])
        .assert()
        .success();
}

fn push_json(artifact: &Path, reference: &str, force: bool) -> Value {
    let mut command = tuff();
    command.args([
        "pack",
        "push",
        artifact.to_str().unwrap(),
        reference,
        "--plain-http",
        "--json",
    ]);
    if force {
        command.arg("--force");
    }
    let assert = command.assert().success();
    serde_json::from_slice(&assert.get_output().stdout).unwrap()
}

#[test]
#[ignore = "requires TUFF_OCI_TEST_REGISTRY to name a disposable OCI registry"]
fn oci_push_pull_round_trip_preserves_bytes_and_safe_tags() {
    let registry = std::env::var("TUFF_OCI_TEST_REGISTRY")
        .expect("TUFF_OCI_TEST_REGISTRY must name the test registry host and port");
    let temp = TempDir::new().unwrap();
    let first = temp.path().join("first.tuffpack");
    let second = temp.path().join("second.tuffpack");
    make_pack(temp.path(), "1.0.0", "# First release\n", &first);
    make_pack(temp.path(), "2.0.0", "# Second release\n", &second);
    let reference = format!(
        "{registry}/tuff-tests/engineering-{}:roundtrip",
        std::process::id()
    );

    let first_push = push_json(&first, &reference, false);
    assert_eq!(first_push["status"], "pushed");
    let first_digest_reference = first_push["reference"].as_str().unwrap().to_string();

    let repeated = push_json(&first, &reference, false);
    assert_eq!(repeated["status"], "unchanged");

    tuff()
        .args([
            "pack",
            "push",
            second.to_str().unwrap(),
            &reference,
            "--plain-http",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "refusing to move existing OCI tag",
        ));

    let second_push = push_json(&second, &reference, true);
    assert_eq!(second_push["status"], "pushed");

    let pulled_tag = temp.path().join("pulled-tag.tuffpack");
    tuff()
        .args([
            "pack",
            "pull",
            &reference,
            "--output",
            pulled_tag.to_str().unwrap(),
            "--plain-http",
        ])
        .assert()
        .success();
    assert_eq!(fs::read(&second).unwrap(), fs::read(&pulled_tag).unwrap());

    let pulled_digest = temp.path().join("pulled-digest.tuffpack");
    tuff()
        .args([
            "pack",
            "pull",
            &first_digest_reference,
            "--output",
            pulled_digest.to_str().unwrap(),
            "--plain-http",
        ])
        .assert()
        .success();
    assert_eq!(fs::read(&first).unwrap(), fs::read(&pulled_digest).unwrap());

    tuff()
        .args(["pack", "verify", pulled_digest.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
#[ignore = "requires TUFF_OCI_TEST_REGISTRY to name a disposable OCI registry"]
fn outdated_resolves_a_newer_pack_version_from_the_registry() {
    let registry = std::env::var("TUFF_OCI_TEST_REGISTRY")
        .expect("TUFF_OCI_TEST_REGISTRY must name the test registry host and port");
    let temp = TempDir::new().unwrap();
    let older = temp.path().join("older.tuffpack");
    let newer = temp.path().join("newer.tuffpack");
    make_pack(temp.path(), "1.0.0", "# Older release\n", &older);
    make_pack(temp.path(), "1.2.0", "# Newer release\n", &newer);
    let repository = format!("{registry}/tuff-tests/outdated-{}", std::process::id());

    push_json(&older, &format!("{repository}:1.0.0"), false);
    push_json(&newer, &format!("{repository}:1.2.0"), false);

    // Install the older release with --reference, so the lockfile records
    // where it came from and outdated has something to check against.
    let project = TempDir::new().unwrap();
    tuff()
        .current_dir(project.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .args([
            "add",
            "pack",
            older.to_str().unwrap(),
            "--agent",
            "open-agents",
            "--reference",
            &format!("{repository}:1.0.0"),
        ])
        .assert()
        .success();

    tuff()
        .current_dir(project.path())
        .args(["outdated", "--plain-http"])
        .assert()
        .success()
        .stdout(predicate::str::contains("registry-skill"))
        .stdout(predicate::str::contains("1.0.0"))
        .stdout(predicate::str::contains("1.2.0"))
        .stdout(predicate::str::contains("outdated"));

    // A pack installed without --reference must stay honest rather than
    // guess: this is the regression #92 fixed. A separate project, since the
    // same capability id cannot be tracked twice in one lockfile.
    let unreferenced_project = TempDir::new().unwrap();
    tuff()
        .current_dir(unreferenced_project.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(unreferenced_project.path())
        .args([
            "add",
            "pack",
            newer.to_str().unwrap(),
            "--agent",
            "open-agents",
        ])
        .assert()
        .success();
    tuff()
        .current_dir(unreferenced_project.path())
        .args(["outdated", "--plain-http"])
        .assert()
        .success()
        .stdout(predicate::str::contains("not checked"))
        .stdout(predicate::str::contains("outdated").not());
}

#[test]
#[ignore = "requires TUFF_OCI_TEST_REGISTRY to name a disposable OCI registry"]
fn update_resolves_and_installs_a_newer_pack_version_from_the_registry() {
    let registry = std::env::var("TUFF_OCI_TEST_REGISTRY")
        .expect("TUFF_OCI_TEST_REGISTRY must name the test registry host and port");
    let temp = TempDir::new().unwrap();
    let older = temp.path().join("older.tuffpack");
    let newer = temp.path().join("newer.tuffpack");
    make_pack(temp.path(), "1.0.0", "# Older release\n", &older);
    make_pack(temp.path(), "1.2.0", "# Newer release\n", &newer);
    let repository = format!("{registry}/tuff-tests/update-{}", std::process::id());

    push_json(&older, &format!("{repository}:1.0.0"), false);
    push_json(&newer, &format!("{repository}:1.2.0"), false);

    let project = TempDir::new().unwrap();
    tuff()
        .current_dir(project.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .args([
            "add",
            "pack",
            older.to_str().unwrap(),
            "--agent",
            "open-agents",
            "--reference",
            &format!("{repository}:1.0.0"),
        ])
        .assert()
        .success();

    // A dry run resolves the tag list but pulls nothing.
    tuff()
        .current_dir(project.path())
        .args(["update", "registry-skill", "--check", "--plain-http"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "pack com.acme/registry-test can be updated 1.0.0 -> 1.2.0 for open-agents",
        ));
    assert_eq!(
        fs::read_to_string(
            project
                .path()
                .join(".agents/skills/registry-skill/SKILL.md")
        )
        .unwrap(),
        "# Older release\n"
    );

    tuff()
        .current_dir(project.path())
        .args(["update", "registry-skill", "--plain-http"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "updated pack com.acme/registry-test 1.0.0 -> 1.2.0",
        ))
        .stdout(predicate::str::contains(format!("from {repository}:1.2.0")));
    assert_eq!(
        fs::read_to_string(
            project
                .path()
                .join(".agents/skills/registry-skill/SKILL.md")
        )
        .unwrap(),
        "# Newer release\n"
    );

    // The registry stays on record, so the next outdated check still works
    // and now reports the installed release as current.
    tuff()
        .current_dir(project.path())
        .args(["outdated", "--plain-http"])
        .assert()
        .success()
        .stdout(predicate::str::contains("registry-skill"))
        .stdout(predicate::str::contains("up to date"));
    tuff()
        .current_dir(project.path())
        .args(["update", "registry-skill", "--plain-http"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "pack com.acme/registry-test is already up to date (1.2.0)",
        ));
    tuff()
        .current_dir(project.path())
        .arg("check")
        .assert()
        .success();
}

#[test]
#[ignore = "requires TUFF_OCI_TEST_REGISTRY to name a disposable OCI registry"]
fn outdated_and_update_detect_a_tag_repointed_to_different_content() {
    let registry = std::env::var("TUFF_OCI_TEST_REGISTRY")
        .expect("TUFF_OCI_TEST_REGISTRY must name the test registry host and port");
    let temp = TempDir::new().unwrap();
    let original = temp.path().join("original.tuffpack");
    let republished = temp.path().join("republished.tuffpack");
    make_pack(temp.path(), "1.0.0", "# As installed\n", &original);
    // Same pack, same version, different bytes: the supply-chain case.
    let republished_source = temp.path().join("republished-src");
    fs::create_dir_all(&republished_source).unwrap();
    make_pack(
        &republished_source,
        "1.0.0",
        "# Quietly changed\n",
        &republished,
    );
    let repository = format!("{registry}/tuff-tests/repoint-{}", std::process::id());
    let tag = format!("{repository}:1.0.0");
    push_json(&original, &tag, false);

    let project = TempDir::new().unwrap();
    tuff()
        .current_dir(project.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .args([
            "add",
            "pack",
            original.to_str().unwrap(),
            "--agent",
            "open-agents",
            "--reference",
            &tag,
        ])
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .args(["outdated", "--plain-http"])
        .assert()
        .success()
        .stdout(predicate::str::contains("up to date"))
        .stdout(predicate::str::contains("repointed").not());

    // Move the tag. --force is the only way a push does this, on purpose.
    push_json(&republished, &tag, true);

    tuff()
        .current_dir(project.path())
        .args(["outdated", "--plain-http"])
        .assert()
        .success()
        .stdout(predicate::str::contains("registry-skill"))
        .stdout(predicate::str::contains("repointed"))
        .stdout(predicate::str::contains("up to date").not());

    // A plain update refuses: "up to date" would be a lie, and silently
    // replacing the install would be worse.
    tuff()
        .current_dir(project.path())
        .args(["update", "registry-skill", "--plain-http"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("was republished"))
        .stderr(predicate::str::contains("--force"));
    assert_eq!(
        fs::read_to_string(
            project
                .path()
                .join(".agents/skills/registry-skill/SKILL.md")
        )
        .unwrap(),
        "# As installed\n"
    );
    tuff()
        .current_dir(project.path())
        .args(["update", "registry-skill", "--check", "--plain-http"])
        .assert()
        .success()
        .stdout(predicate::str::contains("was republished"));

    // Forcing it accepts what the tag serves now and records that digest.
    tuff()
        .current_dir(project.path())
        .args(["update", "registry-skill", "--force", "--plain-http"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "updated pack com.acme/registry-test 1.0.0 -> 1.0.0",
        ));
    assert_eq!(
        fs::read_to_string(
            project
                .path()
                .join(".agents/skills/registry-skill/SKILL.md")
        )
        .unwrap(),
        "# Quietly changed\n"
    );
    tuff()
        .current_dir(project.path())
        .args(["outdated", "--plain-http"])
        .assert()
        .success()
        .stdout(predicate::str::contains("up to date"))
        .stdout(predicate::str::contains("repointed").not());
    tuff()
        .current_dir(project.path())
        .arg("check")
        .assert()
        .success();
}
