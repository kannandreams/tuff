//! `tuff console publish` against a real `tuff console serve` process, and
//! against a fake GitHub Actions runner.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::Command;

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

/// A `tuff console serve` process on a free port, stopped on drop.
struct ConsoleProcess {
    child: std::process::Child,
    url: String,
    // Kept open so the server's later lines do not hit a closed pipe.
    _stdout: BufReader<std::process::ChildStdout>,
}

impl Drop for ConsoleProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start_console(data: &Path, extra: &[&str]) -> ConsoleProcess {
    let mut child = tuff()
        .args(["console", "serve", "--addr", "127.0.0.1:0", "--data"])
        .arg(data)
        .args(extra)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut first = String::new();
    stdout.read_line(&mut first).unwrap();
    let addr = first
        .trim()
        .strip_prefix("Console listening on http://")
        .unwrap_or_else(|| panic!("unexpected first line {first:?}"))
        .to_string();
    ConsoleProcess {
        child,
        url: format!("http://{addr}"),
        _stdout: stdout,
    }
}

/// A repository with two projects, `apps/support-agent` and
/// `apps/billing-agent`, whose origin is `git@github.com:acme/agents.git`.
fn publishable_monorepo(temp: &Path, home: &Path) -> std::path::PathBuf {
    let repo = temp.join("agents");
    for app in ["apps/support-agent", "apps/billing-agent"] {
        let dir = repo.join(app);
        fs::create_dir_all(&dir).unwrap();
        tuff()
            .current_dir(&dir)
            .env("HOME", home)
            .arg("init")
            .assert()
            .success();
    }
    git(&repo, &["init", "-q"]);
    git(
        &repo,
        &["remote", "add", "origin", "git@github.com:acme/agents.git"],
    );
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "init"]);
    repo
}

fn created_key(output: &[u8]) -> String {
    String::from_utf8_lossy(output)
        .lines()
        .find(|line| line.starts_with("tuffc_"))
        .expect("the key is printed")
        .to_string()
}

#[test]
fn publish_sends_each_project_and_reports_what_the_server_did() {
    let temp = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let repo = publishable_monorepo(temp.path(), home.path());
    let console = start_console(data.path(), &[]);

    let publish = || {
        tuff()
            .current_dir(&repo)
            .env("HOME", home.path())
            .env_remove("TUFF_CONSOLE_URL")
            .env_remove("TUFF_CONSOLE_KEY")
            .args(["console", "publish", "--all", "--server", &console.url])
            .assert()
    };
    publish()
        .success()
        .stdout(predicate::str::contains("Publishing 2 projects"))
        .stdout(predicate::str::contains(
            "stored     github.com/acme/agents apps/billing-agent (report 1)",
        ))
        .stdout(predicate::str::contains(
            "stored     github.com/acme/agents apps/support-agent",
        ))
        .stdout(predicate::str::contains("2 stored, 0 unchanged."));
    publish()
        .success()
        .stdout(predicate::str::contains(
            "unchanged  github.com/acme/agents apps/billing-agent",
        ))
        .stdout(predicate::str::contains("0 stored, 2 unchanged."));

    let store = tuff_console::Store::open(data.path()).unwrap();
    assert_eq!(store.projects().unwrap().len(), 2);
    let events = store.events(&tuff_console::EventFilter::default()).unwrap();
    let first_seen = events
        .iter()
        .filter(|event| event.kind == "project_first_seen")
        .count();
    assert_eq!(first_seen, 2);
}

#[test]
fn publish_uses_the_key_from_the_environment_and_exits_non_zero_when_refused() {
    let temp = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let repo = publishable_monorepo(temp.path(), home.path());

    let created = tuff()
        .args(["console", "key", "create", "ci", "--data"])
        .arg(data.path())
        .output()
        .unwrap();
    let key = created_key(&created.stdout);
    let console = start_console(data.path(), &[]);
    let publish = |key: Option<&str>| {
        let mut command = tuff();
        command
            .current_dir(&repo)
            .env("HOME", home.path())
            .env_remove("TUFF_CONSOLE_KEY")
            .env("TUFF_CONSOLE_URL", &console.url)
            .args(["console", "publish", "--all"]);
        if let Some(key) = key {
            command.env("TUFF_CONSOLE_KEY", key);
        }
        command.assert()
    };

    publish(None)
        .failure()
        .stdout(predicate::str::contains("refused"))
        .stdout(predicate::str::contains("HTTP 401"))
        .stderr(predicate::str::contains("refused 2 of 2 projects"));
    publish(Some(&key))
        .success()
        .stdout(predicate::str::contains("with an API key"))
        .stdout(predicate::str::contains("2 stored"));
}

#[test]
fn publish_with_a_key_for_another_repository_is_refused_with_403() {
    let temp = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let repo = publishable_monorepo(temp.path(), home.path());
    let created = tuff()
        .args([
            "console",
            "key",
            "create",
            "other",
            "--repository",
            "github.com/acme/other",
            "--data",
        ])
        .arg(data.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&created.stdout).to_string();
    assert!(
        stdout.contains("only for github.com/acme/other"),
        "{stdout}"
    );
    let key = created_key(&created.stdout);
    let console = start_console(data.path(), &[]);

    tuff()
        .current_dir(&repo)
        .env("HOME", home.path())
        .args(["console", "publish", "--all", "--key", &key, "--server"])
        .arg(&console.url)
        .assert()
        .failure()
        .stdout(predicate::str::contains("HTTP 403"))
        .stdout(predicate::str::contains("github.com/acme/other"));
    let store = tuff_console::Store::open(data.path()).unwrap();
    assert!(store.projects().unwrap().is_empty());

    tuff()
        .args(["console", "key", "list", "--data"])
        .arg(data.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("github.com/acme/other"));
}

#[test]
fn publish_fails_clearly_when_no_console_listens() {
    let temp = TempDir::new().unwrap();
    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    tuff()
        .current_dir(temp.path())
        .env_remove("TUFF_CONSOLE_URL")
        .env_remove("TUFF_CONSOLE_KEY")
        .args([
            "console",
            "publish",
            "--project",
            "local",
            "--server",
            "http://127.0.0.1:9",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot reach http://127.0.0.1:9"))
        .stderr(predicate::str::contains("tuff console serve"));
}

/// A server that answers the runner's token request and accepts one
/// report, recording the head of each request it saw.
fn fake_actions_console() -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut received = Vec::new();
            let mut buffer = [0u8; 8192];
            let header_end = loop {
                let read = stream.read(&mut buffer).unwrap();
                received.extend_from_slice(&buffer[..read]);
                if let Some(at) = received.windows(4).position(|w| w == b"\r\n\r\n") {
                    break at + 4;
                }
            };
            let head = String::from_utf8_lossy(&received[..header_end]).to_string();
            let length = head
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|value| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            while received.len() < header_end + length {
                let read = stream.read(&mut buffer).unwrap();
                received.extend_from_slice(&buffer[..read]);
            }
            let (status, body) = if head.starts_with("GET /token") {
                ("200 OK", r#"{"value":"jwt-from-runner"}"#)
            } else {
                (
                    "201 Created",
                    r#"{"projectId":1,"reportId":1,"deduplicated":false,"projectFirstSeen":true}"#,
                )
            };
            seen.push(head);
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
        seen
    });
    (base, handle)
}

#[test]
fn publish_in_github_actions_sends_the_runner_token() {
    let temp = TempDir::new().unwrap();
    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    let (base, handle) = fake_actions_console();

    tuff()
        .current_dir(temp.path())
        .env_remove("TUFF_CONSOLE_KEY")
        .env(
            "ACTIONS_ID_TOKEN_REQUEST_URL",
            format!("{base}/token?api-version=2"),
        )
        .env("ACTIONS_ID_TOKEN_REQUEST_TOKEN", "runner-secret")
        .args([
            "console",
            "publish",
            "--project",
            "github.com/acme/web",
            "--server",
        ])
        .arg(format!("{base}/"))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "with the GitHub Actions OIDC token",
        ))
        .stdout(predicate::str::contains("stored"));

    let seen = handle.join().unwrap();
    let token_request = seen[0].to_ascii_lowercase();
    assert!(
        token_request.starts_with("get /token?api-version=2&audience="),
        "{seen:?}"
    );
    // The audience is the console URL without its trailing slash.
    let audience = base.trim_start_matches("http://").replace(':', "%3a");
    assert!(
        token_request.contains(&format!("audience=http%3a%2f%2f{audience} ")),
        "{seen:?}"
    );
    assert!(
        token_request.contains("authorization: bearer runner-secret"),
        "{seen:?}"
    );
    let report_request = seen[1].to_ascii_lowercase();
    assert!(
        report_request.starts_with("post /api/v1/reports"),
        "{seen:?}"
    );
    assert!(
        report_request.contains("authorization: bearer jwt-from-runner"),
        "{seen:?}"
    );
}

#[test]
fn a_key_wins_over_the_runner_token() {
    let temp = TempDir::new().unwrap();
    tuff()
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();
    // The token URL is never called, so a dead address is enough.
    tuff()
        .current_dir(temp.path())
        .env("ACTIONS_ID_TOKEN_REQUEST_URL", "http://127.0.0.1:9/token")
        .env("ACTIONS_ID_TOKEN_REQUEST_TOKEN", "runner-secret")
        .args([
            "console",
            "publish",
            "--project",
            "local",
            "--key",
            "tuffc_x",
            "--server",
            "http://127.0.0.1:9",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot reach http://127.0.0.1:9"));
}

#[test]
fn serve_validates_trusts_and_a_trust_satisfies_the_public_bind_rule() {
    let data = TempDir::new().unwrap();
    tuff()
        .args(["console", "serve", "--trust", "gitlab:acme", "--data"])
        .arg(data.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("only 'github' is supported"));
    tuff()
        .args(["console", "serve", "--trust", "acme", "--data"])
        .arg(data.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("--trust github:acme"));
    tuff()
        .args([
            "console",
            "serve",
            "--public-url",
            "tuff.internal",
            "--data",
        ])
        .arg(data.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not an http(s) URL"));

    // A trust is something configured, so a public bind needs no key.
    let mut server = tuff()
        .args([
            "console",
            "serve",
            "--addr",
            "0.0.0.0:0",
            "--public-read",
            "--trust",
            "github:acme",
            "--public-url",
            "https://tuff.internal.acme.dev/",
            "--data",
        ])
        .arg(data.path())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(server.stdout.take().unwrap()).lines();
    let mut output = String::new();
    for _ in 0..4 {
        output.push_str(&lines.next().unwrap().unwrap());
        output.push('\n');
    }
    let _ = server.kill();
    let _ = server.wait();
    assert!(
        output.contains("Trusting GitHub Actions jobs of: github:acme"),
        "{output}"
    );
    assert!(
        output.contains("OIDC audience: https://tuff.internal.acme.dev\n"),
        "{output}"
    );
}
