//! Packs written as Claude Code plugins and marketplaces (RFC-110
//! milestone 4), and `{{hook_dir}}` in standard hook commands.

use std::{fs, path::Path, process::Command};

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

fn tuff() -> Command {
    Command::cargo_bin("tuff").unwrap()
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A pack with two skills, an MCP server, and a hook that runs a script it
/// ships through `{{hook_dir}}`.
fn write_pack(root: &Path, name: &str, targets: &str) -> std::path::PathBuf {
    let pack = root.join(name.rsplit('/').next().unwrap());
    write(
        &pack.join("tuff-pack.toml"),
        &format!(
            r#"schema = 1
name = "{name}"
version = "1.4.0"
description = "Review helpers."
homepage = "https://example.com/review"
license = "MIT"
keywords = ["review"]

[author]
name = "Acme"
url = "https://example.com"

[build]
targets = [{targets}]

[[capabilities]]
path = "capabilities/review"

[[capabilities]]
path = "capabilities/explain"

[[capabilities]]
path = "capabilities/github"

[[capabilities]]
path = "capabilities/lint"
"#
        ),
    );
    write(
        &pack.join("capabilities/review/tuff.toml"),
        "id = \"review\"\nversion = \"1.4.0\"\ntype = \"skill\"\ndescription = \"Review a pull request.\"\nfiles = [\"SKILL.md\", \"scripts/run.sh\"]\n",
    );
    write(
        &pack.join("capabilities/review/SKILL.md"),
        "---\nname: review\ndescription: Review a pull request.\n---\n\n# Review\n",
    );
    write(
        &pack.join("capabilities/review/scripts/run.sh"),
        "echo review\n",
    );
    write(
        &pack.join("capabilities/explain/tuff.toml"),
        "id = \"explain\"\nversion = \"1.4.0\"\ntype = \"skill\"\ndescription = \"Explain code.\"\nfiles = [\"SKILL.md\"]\n",
    );
    write(
        &pack.join("capabilities/explain/SKILL.md"),
        "---\nname: explain\ndescription: Explain code.\n---\n\n# Explain\n",
    );
    write(
        &pack.join("capabilities/github/tuff.toml"),
        r#"id = "github"
version = "1.4.0"
type = "mcp-server"
description = "GitHub."

[server]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[server.env]
GITHUB_TOKEN = { from_env = "GITHUB_TOKEN" }
"#,
    );
    write(
        &pack.join("capabilities/lint/tuff.toml"),
        r#"id = "lint"
version = "1.4.0"
type = "hook"
description = "Lint after edits."
files = ["lint.sh"]

[hook]
event = "post_tool_use"
command = "sh {{hook_dir}}/lint.sh"
"#,
    );
    write(&pack.join("capabilities/lint/lint.sh"), "echo lint\n");
    pack
}

fn build(pack: &Path, artifact: &Path, extra: &[&str]) {
    tuff()
        .args([
            "pack",
            "build",
            pack.to_str().unwrap(),
            "--output",
            artifact.to_str().unwrap(),
        ])
        .args(extra)
        .assert()
        .success();
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

/// `claude plugin validate`, when Claude Code is installed. CI without it
/// still checks the layout by hand.
fn claude_validate(path: &Path) {
    let Ok(output) = Command::new("claude")
        .args(["plugin", "validate", path.to_str().unwrap()])
        .output()
    else {
        eprintln!("claude is not installed; skipping 'claude plugin validate'");
        return;
    };
    assert!(
        output.status.success(),
        "claude plugin validate failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_pack_builds_as_a_claude_code_plugin_and_imports_back() {
    let author = TempDir::new().unwrap();
    let pack = write_pack(
        author.path(),
        "acme/review-kit",
        r#""open-agents", "claude-plugin""#,
    );
    let artifact = author.path().join("review-kit.tuffpack");
    build(&pack, &artifact, &[]);
    let plugin = author.path().join("plugin");
    tuff()
        .args([
            "pack",
            "extract",
            artifact.to_str().unwrap(),
            "-a",
            "claude-plugin",
            "--output",
            plugin.to_str().unwrap(),
        ])
        .assert()
        .success();

    let manifest = read_json(&plugin.join(".claude-plugin/plugin.json"));
    assert_eq!(manifest["name"], "review-kit");
    assert_eq!(manifest["version"], "1.4.0");
    assert_eq!(manifest["author"]["name"], "Acme");
    assert_eq!(manifest["license"], "MIT");
    assert_eq!(manifest["keywords"][0], "review");
    assert_eq!(
        fs::read_to_string(plugin.join("skills/review/scripts/run.sh")).unwrap(),
        "echo review\n"
    );
    assert!(plugin.join("skills/explain/SKILL.md").is_file());
    let mcp = read_json(&plugin.join(".mcp.json"));
    assert_eq!(mcp["mcpServers"]["github"]["command"], "npx");
    assert_eq!(
        mcp["mcpServers"]["github"]["env"]["GITHUB_TOKEN"],
        "${GITHUB_TOKEN}"
    );
    let hooks = read_json(&plugin.join("hooks/hooks.json"));
    assert_eq!(
        hooks["hooks"]["PostToolUse"][0]["hooks"][0]["command"],
        "sh ${CLAUDE_PLUGIN_ROOT}/hooks/lint/lint.sh"
    );
    assert_eq!(
        fs::read_to_string(plugin.join("hooks/lint/lint.sh")).unwrap(),
        "echo lint\n"
    );
    claude_validate(&plugin);

    // Building twice gives the same bytes.
    let again = author.path().join("again.tuffpack");
    build(&pack, &again, &[]);
    assert_eq!(fs::read(&artifact).unwrap(), fs::read(&again).unwrap());

    // Reading the plugin back gives the same skills and server. Hooks are
    // not imported, so the hook comes back as unsupported.
    let imported = author.path().join("imported");
    tuff()
        .args([
            "pack",
            "import",
            plugin.to_str().unwrap(),
            "--output",
            imported.to_str().unwrap(),
        ])
        .assert()
        .success();
    for file in [
        "review/SKILL.md",
        "review/scripts/run.sh",
        "explain/SKILL.md",
    ] {
        assert_eq!(
            fs::read(imported.join("capabilities").join(file)).unwrap(),
            fs::read(pack.join("capabilities").join(file)).unwrap(),
            "{file}"
        );
    }
    let server = |folder: &Path| {
        let manifest =
            tuff_core::manifest::load_manifest(&folder.join("capabilities/github")).unwrap();
        serde_json::to_value(manifest.server.unwrap()).unwrap()
    };
    assert_eq!(server(&pack), server(&imported));
    let manifest = fs::read_to_string(imported.join("tuff-pack.toml")).unwrap();
    assert!(manifest.contains("kind = \"hooks\""), "{manifest}");
    assert!(manifest.contains("name = \"review-kit\""), "{manifest}");
    assert!(manifest.contains("version = \"1.4.0\""), "{manifest}");

    let document: serde_json::Value = serde_json::from_slice(
        &tuff()
            .args(["pack", "inspect", artifact.to_str().unwrap(), "--json"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let review = document["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|member| member["id"] == "review")
        .unwrap();
    assert_eq!(
        review["files"]["claude-plugin"],
        serde_json::json!(["skills/review/SKILL.md", "skills/review/scripts/run.sh"])
    );
}

#[test]
fn a_member_a_plugin_cannot_hold_fails_the_build_unless_skipped() {
    let author = TempDir::new().unwrap();
    let pack = write_pack(author.path(), "acme/review-kit", r#""claude-plugin""#);
    write(
        &pack.join("capabilities/runner/tuff.toml"),
        r#"id = "runner"
version = "1.0.0"
type = "tool"
description = "A tool that is not an MCP server."
files = ["run.sh"]

[parameters]
type = "object"

[parameters.properties.target]
type = "string"

[implementation]
language = "bash"
entrypoint = "run.sh"
"#,
    );
    write(&pack.join("capabilities/runner/run.sh"), "echo run\n");
    let manifest = fs::read_to_string(pack.join("tuff-pack.toml")).unwrap()
        + "\n[[capabilities]]\npath = \"capabilities/runner\"\n";
    fs::write(pack.join("tuff-pack.toml"), manifest).unwrap();

    let artifact = author.path().join("out.tuffpack");
    tuff()
        .args([
            "pack",
            "build",
            pack.to_str().unwrap(),
            "--output",
            artifact.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "the claude-plugin target cannot hold tool 'runner'",
        ))
        .stderr(predicate::str::contains("--skip-unsupported"));
    assert!(!artifact.exists());

    tuff()
        .args([
            "pack",
            "build",
            pack.to_str().unwrap(),
            "--output",
            artifact.to_str().unwrap(),
            "--skip-unsupported",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "claude-plugin leaves out tool 'runner'",
        ));
    tuff()
        .args(["pack", "verify", artifact.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn a_bundle_target_is_extracted_and_never_installed() {
    let author = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let pack = write_pack(
        author.path(),
        "acme/review-kit",
        r#""claude", "claude-plugin""#,
    );
    let artifact = author.path().join("review-kit.tuffpack");
    build(&pack, &artifact, &[]);
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
        .args([
            "add",
            "pack",
            artifact.to_str().unwrap(),
            "-a",
            "claude-plugin",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("claude-plugin is a bundle format"))
        .stderr(predicate::str::contains("tuff pack extract"));

    // The same pack installs into the project for Claude Code, where
    // {{hook_dir}} names the hook's folder in .claude/.
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .args(["add", "pack", artifact.to_str().unwrap(), "-a", "claude"])
        .assert()
        .success();
    let script = fs::read_to_string(project.path().join(".claude/hooks/lint/run.sh")).unwrap();
    assert!(script.contains("sh .claude/hooks/lint/lint.sh"), "{script}");
    assert!(project.path().join(".claude/hooks/lint/lint.sh").is_file());
    tuff()
        .current_dir(project.path())
        .env("HOME", home.path())
        .arg("check")
        .assert()
        .success();
}

#[test]
fn packs_build_as_a_marketplace_that_imports_back() {
    let author = TempDir::new().unwrap();
    let packs = author.path().join("packs");
    write_pack(&packs, "acme/review-kit", r#""open-agents""#);
    let second = write_pack(&packs, "acme/explain-kit", r#""open-agents""#);
    // The second pack is passed as a built artifact with the plugin target.
    let artifact = author.path().join("explain-kit.tuffpack");
    let manifest = fs::read_to_string(second.join("tuff-pack.toml"))
        .unwrap()
        .replace(
            r#"targets = ["open-agents"]"#,
            r#"targets = ["open-agents", "claude-plugin"]"#,
        );
    fs::write(second.join("tuff-pack.toml"), manifest).unwrap();
    build(&second, &artifact, &[]);
    fs::remove_dir_all(&second).unwrap();

    let market = author.path().join("acme-tools");
    tuff()
        .args([
            "pack",
            "marketplace",
            "build",
            packs.to_str().unwrap(),
            artifact.to_str().unwrap(),
            "--output",
            market.to_str().unwrap(),
            "--owner-name",
            "Acme",
            "--owner-url",
            "https://example.com",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "built marketplace acme-tools with 2 plugin(s)",
        ));
    let listing = read_json(&market.join(".claude-plugin/marketplace.json"));
    assert_eq!(listing["name"], "acme-tools");
    assert_eq!(listing["owner"]["name"], "Acme");
    let plugins = listing["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 2);
    assert_eq!(plugins[0]["name"], "review-kit");
    assert_eq!(plugins[0]["source"], "./plugins/review-kit");
    assert_eq!(plugins[0]["version"], "1.4.0");
    assert_eq!(plugins[1]["name"], "explain-kit");
    assert!(
        market
            .join("plugins/explain-kit/.claude-plugin/plugin.json")
            .is_file()
    );
    claude_validate(&market);

    let imported = author.path().join("imported");
    tuff()
        .args([
            "pack",
            "import",
            market.to_str().unwrap(),
            "--plugin",
            "review-kit",
            "--output",
            imported.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("imported review-kit 1.4.0"));

    tuff()
        .args([
            "pack",
            "marketplace",
            "build",
            packs.to_str().unwrap(),
            "--output",
            market.to_str().unwrap(),
            "--owner-name",
            "Acme",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must be missing or empty"));
}
