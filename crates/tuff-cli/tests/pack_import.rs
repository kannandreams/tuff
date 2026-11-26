//! Claude Code plugins read as packs (RFC-110 milestone 3): `tuff pack
//! import`, marketplaces, and installs straight from a plugin's repository.

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

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A plugin with two skills, MCP servers Tuff can and cannot carry, and
/// components Tuff does not have.
fn write_plugin(root: &Path, version: &str, review_text: &str) {
    write(
        &root.join(".claude-plugin/plugin.json"),
        &format!(
            r#"{{
  "name": "review-kit",
  "version": "{version}",
  "description": "Review helpers.",
  "author": {{ "name": "Acme", "url": "https://example.com" }},
  "homepage": "https://example.com/review-kit",
  "license": "MIT",
  "keywords": ["review"],
  "mcpServers": {{
    "remote-api": {{
      "type": "http",
      "url": "https://api.example.com/mcp",
      "headers": {{ "Authorization": "Bearer ${{API_TOKEN}}" }}
    }}
  }}
}}
"#
        ),
    );
    write(
        &root.join("skills/review/SKILL.md"),
        &format!("---\nname: review\ndescription: Review a pull request.\n---\n\n{review_text}"),
    );
    write(&root.join("skills/review/scripts/run.sh"), "echo review\n");
    write(&root.join("skills/explain/SKILL.md"), "# Explain\n");
    write(
        &root.join(".mcp.json"),
        r#"{
  "mcpServers": {
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": { "GITHUB_PERSONAL_ACCESS_TOKEN": "${GITHUB_TOKEN}" }
    },
    "bundled": { "command": "node", "args": ["${CLAUDE_PLUGIN_ROOT}/server.js"] },
    "literal": { "command": "tool", "env": { "KEY": "abc123" } },
    "events": { "type": "sse", "url": "https://events.example.com" }
  }
}
"#,
    );
    write(&root.join("commands/hello.md"), "Say hello.\n");
    write(
        &root.join("agents/reviewer.md"),
        "---\nname: reviewer\n---\n",
    );
    write(&root.join("hooks/hooks.json"), r#"{"hooks":{}}"#);
    write(&root.join("server.js"), "console.log('server')\n");
    write(&root.join("README.md"), "# Review kit\n");
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

fn kinds(document: &serde_json::Value) -> Vec<String> {
    document["unsupported"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            format!(
                "{}:{}",
                item["kind"].as_str().unwrap(),
                item["name"].as_str().unwrap_or("")
            )
        })
        .collect()
}

#[test]
fn a_plugin_imports_its_skills_and_mcp_servers_and_keeps_the_rest() {
    let author = TempDir::new().unwrap();
    let plugin = author.path().join("review-kit");
    write_plugin(&plugin, "1.2.0", "Read the diff.\n");
    let output = author.path().join("imported");

    tuff()
        .args([
            "pack",
            "import",
            plugin.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("imported review-kit 1.2.0"))
        .stdout(predicate::str::contains("not carried:"))
        .stdout(predicate::str::contains(
            "commands (commands): Tuff has no slash-command capability",
        ))
        .stdout(predicate::str::contains("bundled in the plugin"))
        // Codex passes a variable to a server only under its own name, and
        // the github server reads GITHUB_TOKEN as GITHUB_PERSONAL_ACCESS_TOKEN.
        .stderr(predicate::str::contains("the pack is not built for Codex"));

    let manifest = fs::read_to_string(output.join("tuff-pack.toml")).unwrap();
    assert!(manifest.contains("name = \"review-kit\""), "{manifest}");
    assert!(
        manifest.contains("format = \"claude-plugin\""),
        "{manifest}"
    );
    assert_eq!(
        fs::read_to_string(output.join("capabilities/review/scripts/run.sh")).unwrap(),
        "echo review\n"
    );
    let github = fs::read_to_string(output.join("capabilities/github/tuff.toml")).unwrap();
    assert!(github.contains("from_env = \"GITHUB_TOKEN\""), "{github}");
    let remote = fs::read_to_string(output.join("capabilities/remote-api/tuff.toml")).unwrap();
    assert!(remote.contains("format = \"Bearer {}\""), "{remote}");
    assert!(remote.contains("from_env = \"API_TOKEN\""), "{remote}");
    for kept in [
        "extra/commands/hello.md",
        "extra/agents/reviewer.md",
        "extra/hooks/hooks.json",
        "extra/.claude-plugin/plugin.json",
        "extra/.mcp.json",
        "extra/server.js",
    ] {
        assert!(output.join(kept).is_file(), "{kept}");
    }
    assert!(!output.join("extra/skills/review/SKILL.md").exists());

    tuff()
        .args(["pack", "check", output.to_str().unwrap()])
        .assert()
        .success();

    let document = json(tuff().args(["pack", "inspect", output.to_str().unwrap(), "--json"]));
    assert_eq!(document["import"]["format"], "claude-plugin");
    assert_eq!(document["author"]["name"], "Acme");
    let members = document["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|member| member["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(members, ["explain", "github", "remote-api", "review"]);
    let kinds = kinds(&document);
    for expected in [
        "commands:",
        "agents:",
        "hooks:",
        "mcp-server:bundled",
        "mcp-server:literal",
        "mcp-server:events",
    ] {
        assert!(
            kinds.contains(&expected.to_string()),
            "{expected} in {kinds:?}"
        );
    }
    let literal = document["unsupported"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == "literal")
        .unwrap();
    assert!(
        literal["reason"]
            .as_str()
            .unwrap()
            .contains("a secret never lands in a manifest")
    );

    // The output must be missing or empty.
    tuff()
        .args([
            "pack",
            "import",
            plugin.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must be missing or empty"));
}

/// A marketplace repository with one plugin inside it and one in another
/// repository.
fn marketplace(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let external = root.join("external");
    fs::create_dir_all(&external).unwrap();
    git(&external, &["init", "--quiet", "--initial-branch", "main"]);
    write(
        &external.join(".claude-plugin/plugin.json"),
        r#"{ "name": "external", "version": "0.3.0" }"#,
    );
    write(&external.join("skills/notes/SKILL.md"), "# Notes\n");
    git(&external, &["add", "-A"]);
    git(&external, &["commit", "--quiet", "-m", "external"]);

    let market = root.join("market");
    fs::create_dir_all(&market).unwrap();
    git(&market, &["init", "--quiet", "--initial-branch", "main"]);
    write_plugin(
        &market.join("plugins/review-kit"),
        "1.2.0",
        "Read the diff.\n",
    );
    write(
        &market.join(".claude-plugin/marketplace.json"),
        &format!(
            r#"{{
  "name": "acme-tools",
  "owner": {{ "name": "Acme" }},
  "plugins": [
    {{ "name": "review-kit", "source": "./plugins/review-kit" }},
    {{ "name": "external", "source": {{ "source": "url", "url": "file://{}" }} }}
  ]
}}
"#,
            external.display()
        ),
    );
    git(&market, &["add", "-A"]);
    git(&market, &["commit", "--quiet", "-m", "market"]);
    (market, external)
}

#[test]
fn a_marketplace_entry_is_chosen_with_plugin_and_may_live_in_another_repository() {
    let author = TempDir::new().unwrap();
    let (market, external) = marketplace(author.path());

    tuff()
        .args([
            "pack",
            "import",
            market.to_str().unwrap(),
            "--output",
            author.path().join("none").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "lists 2 plugins: review-kit, external",
        ))
        .stderr(predicate::str::contains("--plugin"));

    let output = author.path().join("external-pack");
    tuff()
        .args([
            "pack",
            "import",
            market.to_str().unwrap(),
            "--plugin",
            "external",
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("imported external 0.3.0"));
    let manifest = fs::read_to_string(output.join("tuff-pack.toml")).unwrap();
    assert!(
        manifest.contains(&format!("url = \"file://{}\"", external.display())),
        "{manifest}"
    );
    assert!(manifest.contains("plugin = \"external\""), "{manifest}");
    assert!(manifest.contains("commit = \""), "{manifest}");
}

#[test]
fn a_plugin_installs_from_git_and_updates_when_its_repository_tags_a_release() {
    let author = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let repo = author.path().join("review-kit");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet", "--initial-branch", "main"]);
    write_plugin(&repo, "1.2.0", "Read the diff.\n");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "1.2.0"]);
    git(&repo, &["tag", "v1.2.0"]);
    let url = format!("file://{}", repo.display());

    let project = TempDir::new().unwrap();
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("init")
        .assert()
        .success();
    let installed = json(
        tuff()
            .current_dir(project.path())
            .env("HOME", home.path())
            .args(["add", "pack", &url, "-a", "claude", "--json"]),
    );
    assert_eq!(installed["name"], "review-kit");
    assert_eq!(installed["source"]["format"], "claude-plugin");
    assert_eq!(installed["source"]["tag"], "v1.2.0");
    assert!(kinds(&installed).contains(&"commands:".to_string()));
    assert!(
        project
            .path()
            .join(".claude/skills/review/SKILL.md")
            .is_file()
    );
    let mcp: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(project.path().join(".mcp.json")).unwrap())
            .unwrap();
    assert_eq!(
        mcp["mcpServers"]["github"]["env"]["GITHUB_PERSONAL_ACCESS_TOKEN"],
        "${GITHUB_TOKEN}"
    );
    assert!(mcp["mcpServers"].get("bundled").is_none());
    assert!(!project.path().join(".claude/commands").exists());

    write_plugin(&repo, "1.3.0", "Read the diff twice.\n");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "1.3.0"]);
    git(&repo, &["tag", "v1.3.0"]);
    let outdated = json(
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
            .any(|row| row["id"] == "review"
                && row["status"] == "outdated"
                && row["latest"] == "1.3.0"),
        "{outdated}"
    );
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["update", "review"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "updated pack review-kit 1.2.0 -> 1.3.0",
        ));
    assert!(
        fs::read_to_string(project.path().join(".claude/skills/review/SKILL.md"))
            .unwrap()
            .contains("Read the diff twice.")
    );
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("check")
        .assert()
        .success();
}

#[test]
fn a_marketplace_entry_installs_from_its_own_repository_only() {
    let author = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let (market, _) = marketplace(author.path());
    let url = format!("file://{}", market.display());
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
        .args(["add", "pack", &url, "--plugin", "external", "-a", "claude"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("lives in another repository"));
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args([
            "add",
            "pack",
            &url,
            "--plugin",
            "review-kit",
            "-a",
            "claude",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed pack review-kit 1.2.0"));
    let lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(project.path().join("tuff.lock")).unwrap())
            .unwrap();
    assert_eq!(
        lock["packs"]["review-kit"]["source"]["plugin"],
        "review-kit"
    );
    assert_eq!(
        lock["packs"]["review-kit"]["source"]["format"],
        "claude-plugin"
    );
}

#[test]
fn a_plugin_with_nothing_tuff_carries_is_refused() {
    let author = TempDir::new().unwrap();
    let plugin = author.path().join("commands-only");
    write(
        &plugin.join(".claude-plugin/plugin.json"),
        r#"{ "name": "commands-only" }"#,
    );
    write(&plugin.join("commands/hello.md"), "Say hello.\n");
    tuff()
        .args([
            "pack",
            "import",
            plugin.to_str().unwrap(),
            "--output",
            author.path().join("out").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "has no skills or MCP servers Tuff can carry; it has commands",
        ));
    assert!(!author.path().join("out").exists());
}
