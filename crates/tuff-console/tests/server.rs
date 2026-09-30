//! The server on a free port, driven over HTTP the way `tuff console
//! publish` will drive it.

use std::path::Path;
use std::sync::Arc;

use serde_json::{Value, json};
use tuff_console::{ServerOptions, Store, serve};

struct Running {
    base: String,
    store: Arc<Store>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
    _data: tempfile::TempDir,
}

impl Running {
    async fn start(require_key: bool) -> Self {
        Self::start_with(ServerOptions {
            require_key,
            ..Default::default()
        })
        .await
    }

    async fn start_with(options: ServerOptions) -> Self {
        let data = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(data.path()).unwrap());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let task = tokio::spawn({
            let store = Arc::clone(&store);
            async move {
                serve(store, listener, options, async {
                    let _ = stopped.await;
                })
                .await
                .unwrap();
            }
        });
        Self {
            base,
            store,
            stop: Some(stop),
            task,
            _data: data,
        }
    }

    async fn shut_down(mut self) {
        let _ = self.stop.take().unwrap().send(());
        self.task.await.unwrap();
    }
}

/// A report built by the code the CLI uses, from a project folder.
fn build_report(dir: &Path, commit: &str) -> Value {
    std::fs::write(dir.join("tuff.lock"), "{\"version\":3,\"capabilities\":[]}").unwrap();
    let report = tuff_core::report::build_report(dir, Some("acme/agents"), None).unwrap();
    let mut report = serde_json::to_value(report).unwrap();
    report["project"]["commit"] = json!(commit);
    report
}

#[tokio::test]
async fn a_published_report_is_read_back() {
    let server = Running::start(false).await;
    let project = tempfile::tempdir().unwrap();
    let report = build_report(project.path(), "one");
    let client = reqwest::Client::new();

    assert_eq!(
        client
            .get(format!("{}/healthz", server.base))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    let response = client
        .post(format!("{}/api/v1/reports", server.base))
        .json(&report)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let outcome: Value = response.json().await.unwrap();
    assert_eq!(outcome["deduplicated"], false);
    assert_eq!(outcome["projectFirstSeen"], true);
    let id = outcome["projectId"].as_i64().unwrap();

    let projects: Value = client
        .get(format!("{}/api/v1/projects", server.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(projects["projects"].as_array().unwrap().len(), 1);
    assert_eq!(projects["projects"][0]["repository"], "acme/agents");
    assert_eq!(projects["projects"][0]["reportCount"], 1);

    let read: Value = client
        .get(format!("{}/api/v1/projects/{id}", server.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(read["project"]["path"], ".");
    assert_eq!(read["latestReport"], report);

    let missing = client
        .get(format!("{}/api/v1/projects/999", server.base))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
    let error: Value = missing.json().await.unwrap();
    assert_eq!(error["error"]["kind"], "not_found");

    server.shut_down().await;
}

#[tokio::test]
async fn publishing_the_same_report_again_adds_no_row() {
    let server = Running::start(false).await;
    let project = tempfile::tempdir().unwrap();
    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/reports", server.base);

    let first = build_report(project.path(), "same");
    // Reports differ in generatedAt only when a second boundary passes.
    let mut second = first.clone();
    second["generatedAt"] = json!("2099-01-01T00:00:00Z");

    let created = client.post(&url).json(&first).send().await.unwrap();
    assert_eq!(created.status(), 201);
    let repeated = client.post(&url).json(&second).send().await.unwrap();
    assert_eq!(repeated.status(), 200);
    let outcome: Value = repeated.json().await.unwrap();
    assert_eq!(outcome["deduplicated"], true);

    let changed = build_report(project.path(), "different");
    let stored = client.post(&url).json(&changed).send().await.unwrap();
    assert_eq!(stored.status(), 201);

    assert_eq!(server.store.projects().unwrap()[0].report_count, 2);
    server.shut_down().await;
}

#[tokio::test]
async fn publishing_needs_a_live_key_when_required() {
    let server = Running::start(true).await;
    let project = tempfile::tempdir().unwrap();
    let report = build_report(project.path(), "auth");
    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/reports", server.base);
    let secret = server.store.create_key("ci", None).unwrap();

    let anonymous = client.post(&url).json(&report).send().await.unwrap();
    assert_eq!(anonymous.status(), 401);
    assert_eq!(anonymous.headers()["www-authenticate"], "Bearer");
    let error: Value = anonymous.json().await.unwrap();
    assert_eq!(error["error"]["kind"], "unauthorized");
    assert!(error["error"]["hint"].as_str().unwrap().contains("Bearer"));

    let wrong = client
        .post(&url)
        .bearer_auth("tuffc_wrong")
        .json(&report)
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), 401);

    let accepted = client
        .post(&url)
        .bearer_auth(&secret)
        .json(&report)
        .send()
        .await
        .unwrap();
    assert_eq!(accepted.status(), 201);

    server.store.revoke_key("ci").unwrap();
    let revoked = client
        .post(&url)
        .bearer_auth(&secret)
        .json(&report)
        .send()
        .await
        .unwrap();
    assert_eq!(revoked.status(), 401);

    // Reading stays open, as D5 leaves viewer authentication to a proxy.
    let projects = client
        .get(format!("{}/api/v1/projects", server.base))
        .send()
        .await
        .unwrap();
    assert_eq!(projects.status(), 200);

    server.shut_down().await;
}

#[tokio::test]
async fn bad_reports_are_rejected_with_a_reason() {
    let server = Running::start(false).await;
    let project = tempfile::tempdir().unwrap();
    let report = build_report(project.path(), "bad");
    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/reports", server.base);

    let not_json = client.post(&url).body("{oops").send().await.unwrap();
    assert_eq!(not_json.status(), 400);

    let mut future_schema = report.clone();
    future_schema["schema"] = json!(2);
    let response = client.post(&url).json(&future_schema).send().await.unwrap();
    assert_eq!(response.status(), 422);
    let error: Value = response.json().await.unwrap();
    assert_eq!(error["error"]["kind"], "unsupported");
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("schema 2")
    );

    let mut no_project = report.clone();
    no_project["project"] = json!({ "repository": "" });
    let response = client.post(&url).json(&no_project).send().await.unwrap();
    assert_eq!(response.status(), 422);

    let mut blank = report;
    blank["project"]["repository"] = json!(" ");
    let response = client.post(&url).json(&blank).send().await.unwrap();
    assert_eq!(response.status(), 422);

    assert!(server.store.projects().unwrap().is_empty());
    server.shut_down().await;
}

#[tokio::test]
async fn a_loopback_server_with_a_key_refuses_unauthenticated_publishing() {
    // Loopback without --public-read: `require_key` is false, as `run` sets it.
    let server = Running::start(false).await;
    let project = tempfile::tempdir().unwrap();
    let report = build_report(project.path(), "proxy");
    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/reports", server.base);

    // With no key the local server stays open.
    let open = client.post(&url).json(&report).send().await.unwrap();
    assert_eq!(open.status(), 201);

    let secret = server.store.create_key("ci", None).unwrap();
    let mut changed = report.clone();
    changed["project"]["commit"] = json!("later");

    let refused = client.post(&url).json(&changed).send().await.unwrap();
    assert_eq!(refused.status(), 401);
    let accepted = client
        .post(&url)
        .bearer_auth(&secret)
        .json(&changed)
        .send()
        .await
        .unwrap();
    assert_eq!(accepted.status(), 201);

    // Revoking the last key opens publishing again.
    server.store.revoke_key("ci").unwrap();
    let reopened = client.post(&url).json(&report).send().await.unwrap();
    assert_eq!(reopened.status(), 201);

    server.shut_down().await;
}
