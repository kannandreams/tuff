//! `tuff console serve --demo`.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

fn tuff() -> Command {
    Command::cargo_bin("tuff").unwrap()
}

#[test]
fn demo_excludes_data() {
    tuff()
        .args(["console", "serve", "--demo", "--data", "x"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

/// Starts `tuff console serve` with `args` and returns the server, its
/// address, and the second line it printed.
fn start(args: &[&str], envs: &[(&str, &str)]) -> (std::process::Child, String, String) {
    let mut server = tuff()
        .args(["console", "serve"])
        .args(args)
        .envs(envs.iter().copied())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(server.stdout.take().unwrap()).lines();
    let first = lines.next().unwrap().unwrap();
    let second = lines.next().unwrap().unwrap();
    let port = first.rsplit(':').next().unwrap().to_string();
    (server, format!("127.0.0.1:{port}"), second)
}

fn request(addr: &str, method: &str, path: &str) -> String {
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

#[test]
fn demo_serves_sample_projects_from_memory() {
    let (mut server, addr, second) = start(&["--demo", "--addr", "127.0.0.1:0"], &[]);
    let response = request(&addr, "GET", "/api/v1/projects");
    let _ = server.kill();
    let _ = server.wait();

    assert!(second.contains("sample data"), "{second}");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("\"name\":\"payments-api\""), "{response}");
}

#[test]
fn a_public_demo_needs_no_credential_and_refuses_publishing() {
    let (mut server, addr, _) = start(&["--demo", "--addr", "0.0.0.0:0", "--public-read"], &[]);
    let projects = request(&addr, "GET", "/api/v1/projects");
    let publish = request(&addr, "POST", "/api/v1/reports");
    let _ = server.kill();
    let _ = server.wait();

    assert!(projects.starts_with("HTTP/1.1 200"), "{projects}");
    assert!(publish.starts_with("HTTP/1.1 401"), "{publish}");
}

#[test]
fn a_public_demo_still_needs_public_read() {
    tuff()
        .args(["console", "serve", "--demo", "--addr", "0.0.0.0:0"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--public-read"));
}

#[test]
fn the_data_variable_does_not_conflict_with_demo() {
    let (mut server, addr, second) = start(
        &["--demo", "--addr", "127.0.0.1:0"],
        &[("TUFF_CONSOLE_DATA", "/nonexistent/tuff-console-data")],
    );
    let response = request(&addr, "GET", "/healthz");
    let _ = server.kill();
    let _ = server.wait();

    assert!(second.contains("sample data"), "{second}");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
}

#[test]
fn key_commands_use_the_data_variable() {
    let data = tempfile::tempdir().unwrap();
    tuff()
        .args(["console", "key", "create", "ci"])
        .env("TUFF_CONSOLE_DATA", data.path())
        .assert()
        .success();
    assert!(data.path().join("console.sqlite").exists());
    tuff()
        .args(["console", "key", "list"])
        .env("TUFF_CONSOLE_DATA", data.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("ci"));
}
