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
