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

#[test]
fn demo_serves_sample_projects_from_memory() {
    let mut server = tuff()
        .args(["console", "serve", "--demo", "--addr", "127.0.0.1:0"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(server.stdout.take().unwrap()).lines();
    let first = lines.next().unwrap().unwrap();
    let second = lines.next().unwrap().unwrap();
    let addr = first
        .strip_prefix("Console listening on http://")
        .unwrap()
        .to_string();

    let mut stream = std::net::TcpStream::connect(&addr).unwrap();
    write!(
        stream,
        "GET /api/v1/projects HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let _ = server.kill();
    let _ = server.wait();

    assert!(second.contains("sample data"), "{second}");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("\"name\":\"payments-api\""), "{response}");
}
