//! Host tables and `tuff pack inspect --json` (RFC-110 milestone 2).

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

const PACK_MANIFEST: &str = r#"schema = 1
name = "com.acme/review"
version = "1.0.0"
description = "Review capabilities."
homepage = "https://example.com/review"
license = "MIT"
keywords = ["review", "github"]

[author]
name = "Acme"
url = "https://example.com"

[build]
targets = ["open-agents", "claude"]

[[capabilities]]
path = "capabilities/review-skill"

[[capabilities]]
path = "capabilities/github"

[[capabilities]]
path = "capabilities/lint-hook"

[x.host]
default_level = "read"
screens = [{ resource = "ui://changelog/preview", may_call = ["list_pull_requests"] }]
# comments and spacing survive, since the file is kept as written
since   = 2026-11-26
"#;

/// A pack with a skill, a stdio MCP server, an HTTP MCP server, and a hook,
/// with host tables on the pack and on one member.
fn write_pack(root: &Path) -> std::path::PathBuf {
    let pack = root.join("review-pack");
    let skill = pack.join("capabilities/review-skill");
    let github = pack.join("capabilities/github");
    let hook = pack.join("capabilities/lint-hook");
    for dir in [&skill, &github, &hook] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(pack.join("tuff-pack.toml"), PACK_MANIFEST).unwrap();
    fs::write(
        skill.join("tuff.toml"),
        r#"id = "review-skill"
version = "1.0.0"
type = "skill"
description = "How to review."
files = ["SKILL.md", "scripts/check.sh"]

[x.host]
default_level = "write"
"#,
    )
    .unwrap();
    fs::write(skill.join("SKILL.md"), "# Review\n").unwrap();
    fs::create_dir_all(skill.join("scripts")).unwrap();
    fs::write(skill.join("scripts/check.sh"), "echo check\n").unwrap();
    fs::write(
        github.join("tuff.toml"),
        r#"id = "github"
version = "2.0.0"
type = "mcp-server"
description = "GitHub."

[server]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[server.env]
GITHUB_TOKEN = { from_env = "GITHUB_TOKEN" }
"#,
    )
    .unwrap();
    fs::write(
        hook.join("tuff.toml"),
        r#"id = "lint-hook"
version = "1.0.0"
type = "hook"
description = "Lint after edits."

[hook]
event = "post_tool_use"
command = "echo lint"
"#,
    )
    .unwrap();
    pack
}

fn json(command: &mut Command) -> serde_json::Value {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn member<'a>(document: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    document["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|member| member["id"] == id)
        .unwrap()
}

#[test]
fn inspect_json_describes_what_each_member_writes_and_runs() {
    let author = TempDir::new().unwrap();
    let pack = write_pack(author.path());
    let document = json(tuff().args(["pack", "inspect", pack.to_str().unwrap(), "--json"]));

    assert_eq!(document["schema"], 1);
    assert_eq!(document["name"], "com.acme/review");
    assert_eq!(document["homepage"], "https://example.com/review");
    assert_eq!(document["license"], "MIT");
    assert_eq!(
        document["keywords"],
        serde_json::json!(["review", "github"])
    );
    assert_eq!(document["author"]["name"], "Acme");
    assert_eq!(document["source"]["kind"], "folder");
    assert_eq!(document["artifactVersion"], 2);
    assert_eq!(
        document["targets"],
        serde_json::json!(["claude", "open-agents"])
    );
    assert_eq!(document["unsupported"], serde_json::json!([]));
    assert_eq!(document["x"]["host"]["default_level"], "read");
    assert_eq!(document["x"]["host"]["since"], "2026-11-26");
    assert_eq!(
        document["x"]["host"]["screens"][0]["may_call"][0],
        "list_pull_requests"
    );

    let skill = member(&document, "review-skill");
    assert_eq!(skill["runsCode"], false);
    assert_eq!(skill["x"]["host"]["default_level"], "write");
    assert_eq!(
        skill["files"]["claude"],
        serde_json::json!([
            ".claude/skills/review-skill/SKILL.md",
            ".claude/skills/review-skill/scripts/check.sh"
        ])
    );

    let github = member(&document, "github");
    assert_eq!(github["runsCode"], true);
    assert!(
        github["runsCodeReason"]
            .as_str()
            .unwrap()
            .contains("local process")
    );
    assert_eq!(github["mcp"]["transport"], "stdio");
    assert_eq!(github["mcp"]["command"], "npx");
    assert_eq!(github["mcp"]["env"], serde_json::json!(["GITHUB_TOKEN"]));
    assert_eq!(
        github["sharedFiles"]["claude"],
        serde_json::json!([".mcp.json"])
    );

    let hook = member(&document, "lint-hook");
    assert_eq!(hook["runsCode"], true);
    assert_eq!(hook["hook"]["event"], "post_tool_use");
    assert_eq!(
        hook["sharedFiles"]["claude"],
        serde_json::json!([".claude/settings.json"])
    );

    // The text form marks what runs code.
    tuff()
        .args(["pack", "inspect", pack.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "lint-hook 1.0.0 (hook, runs code)",
        ))
        .stdout(predicate::str::contains("review-skill 1.0.0 (skill)\n"));
}

#[test]
fn host_tables_survive_build_and_install_byte_for_byte() {
    let author = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let pack = write_pack(author.path());
    let artifact = author.path().join("review.tuffpack");
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

    let read = tuff_core::pack::read_artifact(&artifact).unwrap();
    let stored = read
        .contents
        .iter()
        .find(|content| content.path == tuff_core::pack::ARTIFACT_MANIFEST_PATH)
        .unwrap();
    assert_eq!(stored.bytes, PACK_MANIFEST.as_bytes());

    let from_artifact =
        json(tuff().args(["pack", "inspect", artifact.to_str().unwrap(), "--json"]));
    assert_eq!(from_artifact["source"]["kind"], "artifact");
    assert_eq!(from_artifact["x"]["host"]["default_level"], "read");
    assert_eq!(
        member(&from_artifact, "review-skill")["x"]["host"]["default_level"],
        "write"
    );

    let project = TempDir::new().unwrap();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["add", "pack", artifact.to_str().unwrap(), "-a", "claude"])
        .assert()
        .success();
    let listing = json(
        tuff()
            .current_dir(project.path())
            .env("HOME", home.path())
            .args(["pack", "list", "--json"]),
    );
    assert_eq!(listing["packs"][0]["x"]["host"]["default_level"], "read");
    let lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(project.path().join("tuff.lock")).unwrap())
            .unwrap();
    assert_eq!(
        lock["packs"]["com.acme/review"]["x"]["host"]["default_level"],
        "read"
    );
}

#[test]
fn inspect_reads_a_pack_from_git_and_reports_the_commit() {
    let author = TempDir::new().unwrap();
    let repo = author.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet", "--initial-branch", "main"]);
    let pack = write_pack(&repo);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "pack"]);
    git(&repo, &["tag", "review/v1.0.0"]);
    let url = format!("file://{}", repo.display());

    let document = json(tuff().args([
        "pack",
        "inspect",
        &url,
        "--path",
        pack.strip_prefix(&repo).unwrap().to_str().unwrap(),
        "--json",
    ]));
    assert_eq!(document["source"]["kind"], "git");
    assert_eq!(document["source"]["tag"], "review/v1.0.0");
    assert_eq!(document["source"]["ref"].as_str().unwrap().len(), 40);
    assert_eq!(document["source"]["path"], "review-pack");
    assert_eq!(member(&document, "github")["runsCode"], true);
}

#[test]
fn a_host_table_namespace_must_be_a_plain_name() {
    let author = TempDir::new().unwrap();
    let pack = write_pack(author.path());
    fs::write(
        pack.join("tuff-pack.toml"),
        PACK_MANIFEST.replace("[x.host]", "[x.\"my host\"]"),
    )
    .unwrap();
    tuff()
        .args(["pack", "check", pack.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("host table namespace 'my host'"));

    fs::write(
        pack.join("tuff-pack.toml"),
        format!("{PACK_MANIFEST}\n[x]\nloose = 1\n").replace("[x.host]", "[x.other]"),
    )
    .unwrap();
    tuff()
        .args(["pack", "check", pack.to_str().unwrap()])
        .assert()
        .failure();
}
