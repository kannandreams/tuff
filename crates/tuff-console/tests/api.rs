//! The read API and the embedded UI, over HTTP, with the demo data. Every
//! endpoint the UI calls has a test here, so a change to a response shape
//! that the UI reads fails in `cargo test`.

use std::sync::Arc;

use serde_json::Value;
use tuff_console::{ServerOptions, Store, demo, serve};

struct Running {
    base: String,
    client: reqwest::Client,
    store: Arc<Store>,
}

async fn start(demo_data: bool, options: ServerOptions) -> Running {
    let store = Arc::new(Store::open_in_memory().unwrap());
    if demo_data {
        demo::populate(&store).unwrap();
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn({
        let store = Arc::clone(&store);
        async move {
            serve(store, listener, options, std::future::pending())
                .await
                .unwrap();
        }
    });
    Running {
        base,
        client: reqwest::Client::new(),
        store,
    }
}

impl Running {
    async fn get(&self, path: &str) -> (u16, Value) {
        let response = self
            .client
            .get(format!("{}{path}", self.base))
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        (status, response.json().await.unwrap_or(Value::Null))
    }

    async fn ok(&self, path: &str) -> Value {
        let (status, body) = self.get(path).await;
        assert_eq!(status, 200, "{path}: {body}");
        body
    }
}

fn id_of(projects: &Value, name: &str) -> i64 {
    projects["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("no project {name}"))["id"]
        .as_i64()
        .unwrap()
}

#[tokio::test]
async fn projects_carry_the_summary_the_dashboard_shows() {
    let console = start(true, ServerOptions::default()).await;
    let body = console.ok("/api/v1/projects").await;

    let summary = &body["summary"];
    for key in [
        "projects",
        "repositories",
        "capabilities",
        "harnesses",
        "driftCount",
        "outdatedCount",
        "policyGapCount",
        "lastReportAt",
    ] {
        assert!(!summary[key].is_null(), "summary.{key}");
    }
    assert_eq!(summary["projects"], 6);
    assert_eq!(summary["repositories"], 5);
    assert_eq!(summary["driftCount"], 2);
    assert_eq!(summary["policyGapCount"], 2);

    let project = &body["projects"][0];
    for key in [
        "id",
        "repository",
        "path",
        "name",
        "firstReportAt",
        "lastReportAt",
        "reportCount",
        "commit",
        "branch",
        "dirty",
        "harnesses",
        "capabilityCount",
        "driftCount",
        "outdatedCount",
        "policyGapCount",
        "status",
    ] {
        assert!(project.get(key).is_some(), "projects[].{key}");
    }
    let statuses: Vec<&str> = body["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["status"].as_str().unwrap())
        .collect();
    for status in ["ok", "drift", "gap", "outdated"] {
        assert!(statuses.contains(&status), "no project is {status}");
    }
}

#[tokio::test]
async fn a_project_has_capabilities_gaps_history_and_a_timeline() {
    let console = start(true, ServerOptions::default()).await;
    let projects = console.ok("/api/v1/projects").await;
    let id = id_of(&projects, "payments-api");

    let detail = console.ok(&format!("/api/v1/projects/{id}")).await;
    assert_eq!(detail["project"]["name"], "payments-api");
    assert!(detail["latestReport"]["lockfile"].is_object());
    let capability = &detail["capabilities"][0];
    for key in [
        "type", "id", "target", "version", "source", "status", "outdated", "latest",
    ] {
        assert!(capability.get(key).is_some(), "capabilities[].{key}");
    }
    assert_eq!(detail["policyGaps"][0]["policy"], "infra-guardrails");
    assert_eq!(detail["policyGaps"][0]["target"], "codex");

    let history = console.ok(&format!("/api/v1/projects/{id}/reports")).await;
    let reports = history["reports"].as_array().unwrap();
    assert_eq!(reports.len(), 3);
    assert!(reports[0]["receivedAt"].as_str() > reports[1]["receivedAt"].as_str());
    for key in [
        "id",
        "receivedAt",
        "generatedAt",
        "commit",
        "branch",
        "tuffVersion",
        "digest",
    ] {
        assert!(reports[0].get(key).is_some(), "reports[].{key}");
    }
    assert!(reports[0].get("body").is_none());

    let (status, error) = console.get("/api/v1/projects/9999/reports").await;
    assert_eq!(status, 404);
    assert_eq!(error["error"]["kind"], "not_found");
}

#[tokio::test]
async fn capabilities_list_versions_and_filter_by_type() {
    let console = start(true, ServerOptions::default()).await;
    let all = console.ok("/api/v1/capabilities").await;
    let list = all["capabilities"].as_array().unwrap();
    let review = list.iter().find(|c| c["id"] == "code-review").unwrap();
    assert_eq!(review["type"], "skill");
    assert_eq!(review["mixed"], true, "code-review is on 2.1.0 and 2.3.0");
    assert!(review["versions"].as_array().unwrap().len() >= 2);
    assert!(review["projects"][0]["name"].is_string());
    assert!(
        list.windows(2)
            .all(|pair| pair[0]["projectCount"].as_u64() >= pair[1]["projectCount"].as_u64())
    );
    let types = all["types"].as_array().unwrap();
    for kind in ["skill", "mcp-server", "policy", "hook", "tool"] {
        assert!(types.iter().any(|t| t == kind), "no {kind}");
    }

    let only = console.ok("/api/v1/capabilities?type=policy").await;
    assert!(
        only["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["type"] == "policy")
    );
    // The type list stays whole, so the filter chips need no second request.
    assert_eq!(only["types"], all["types"]);

    let detail = console.ok("/api/v1/capabilities/skill/code-review").await;
    assert_eq!(detail["mixed"], true);
    let usage = detail["usage"].as_array().unwrap();
    assert!(usage.len() >= 4);
    let outdated = usage.iter().find(|u| u["outdated"] == true).unwrap();
    assert_eq!(outdated["latest"], "2.3.0");
    for key in [
        "projectId",
        "name",
        "repository",
        "path",
        "target",
        "version",
        "source",
        "status",
    ] {
        assert!(usage[0].get(key).is_some(), "usage[].{key}");
    }

    let (status, error) = console.get("/api/v1/capabilities/skill/nothing").await;
    assert_eq!(status, 404);
    assert_eq!(error["error"]["kind"], "not_found");
}

#[tokio::test]
async fn a_capability_id_with_a_slash_is_one_capability() {
    let console = start(false, ServerOptions::default()).await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("tuff.lock"),
        r#"{"version":3,"capabilities":[{"name":"io.example/server","type":"mcp-server","version":"1.0.0","target":"claude","installed_path":".mcp.json","sha256":"x","source":{"kind":"catalog","id":"io.example/server","version":"1.0.0"}}]}"#,
    )
    .unwrap();
    let report = tuff_core::report::build_report(dir.path(), Some("local/x"), None).unwrap();
    let raw = serde_json::to_value(&report).unwrap();
    console.store.ingest(&report, &raw).unwrap();

    let detail = console
        .ok("/api/v1/capabilities/mcp-server/io.example%2Fserver")
        .await;
    assert_eq!(detail["id"], "io.example/server");
    let detail = console
        .ok("/api/v1/capabilities/mcp-server/io.example/server")
        .await;
    assert_eq!(detail["id"], "io.example/server");
}

#[tokio::test]
async fn the_harness_matrix_has_a_count_per_project_and_harness() {
    let console = start(true, ServerOptions::default()).await;
    let body = console.ok("/api/v1/harnesses").await;
    assert_eq!(
        body["harnesses"],
        serde_json::json!(["claude", "codex", "cursor", "opencode", "open-agents"])
    );
    let projects = body["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 6);
    assert!(projects[0]["counts"].is_object());
    assert_eq!(body["totals"]["claude"], 5);
    assert_eq!(body["totals"]["opencode"], 1);
}

#[tokio::test]
async fn policies_list_carriers_gaps_and_projects_without_a_policy() {
    let console = start(true, ServerOptions::default()).await;
    let body = console.ok("/api/v1/policies").await;
    let guard = &body["policies"][0];
    assert_eq!(guard["id"], "infra-guardrails");
    assert_eq!(guard["mixed"], true, "mobile is on 1.1.0");
    assert!(guard["usage"][0]["targets"].is_array());
    assert_eq!(body["gaps"].as_array().unwrap().len(), 2);
    for key in [
        "projectId",
        "name",
        "repository",
        "path",
        "policy",
        "target",
        "rule",
        "description",
        "reason",
    ] {
        assert!(body["gaps"][0].get(key).is_some(), "gaps[].{key}");
    }
    let without: Vec<&str> = body["projectsWithoutPolicy"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert!(without.contains(&"admin"));
    assert!(without.contains(&"data-pipeline"));
}

#[tokio::test]
async fn events_filter_by_project_capability_kind_and_time() {
    let console = start(true, ServerOptions::default()).await;
    let all = console.ok("/api/v1/events?limit=1000").await;
    let events = all["events"].as_array().unwrap();
    for key in [
        "id",
        "projectId",
        "reportId",
        "kind",
        "capabilityType",
        "capabilityId",
        "target",
        "detail",
        "commit",
        "occurredAt",
        "projectName",
        "repository",
        "path",
    ] {
        assert!(events[0].get(key).is_some(), "events[].{key}");
    }
    assert!(all["kinds"].as_array().unwrap().len() >= 10);
    assert!(
        events
            .windows(2)
            .all(|pair| pair[0]["occurredAt"].as_str() >= pair[1]["occurredAt"].as_str())
    );

    let projects = console.ok("/api/v1/projects").await;
    let checkout = id_of(&projects, "checkout");
    let by_project = console
        .ok(&format!("/api/v1/events?project={checkout}"))
        .await;
    assert!(
        by_project["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["projectId"] == checkout)
    );

    let drift = console.ok("/api/v1/events?kind=drift_detected").await;
    let names: Vec<&str> = drift["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["capabilityId"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"format-on-save") && names.contains(&"dbt-docs"));

    let by_capability = console.ok("/api/v1/events?capability=figma").await;
    assert_eq!(by_capability["events"].as_array().unwrap().len(), 1);

    let future = console.ok("/api/v1/events?since=2999-01-01").await;
    assert!(future["events"].as_array().unwrap().is_empty());
    let limited = console.ok("/api/v1/events?limit=2").await;
    assert_eq!(limited["events"].as_array().unwrap().len(), 2);

    for bad in [
        "/api/v1/events?project=abc",
        "/api/v1/events?kind=nonsense",
        "/api/v1/events?since=yesterday",
        "/api/v1/events?limit=many",
    ] {
        let (status, error) = console.get(bad).await;
        assert_eq!(status, 400, "{bad}");
        assert_eq!(error["error"]["kind"], "usage");
    }
}

#[tokio::test]
async fn settings_name_the_server_trusts_and_keys_without_secrets() {
    let options = ServerOptions {
        demo: true,
        public_read: true,
        require_key: true,
        ..Default::default()
    };
    let console = start(true, options).await;
    let secret = console
        .store
        .create_key("ci", Some("github.com/acme/web"))
        .unwrap();
    let body = console.ok("/api/v1/settings").await;

    assert_eq!(body["server"]["demo"], true);
    assert_eq!(body["server"]["loopback"], true);
    assert_eq!(body["server"]["publishRequiresAuth"], true);
    assert!(
        body["server"]["address"]
            .as_str()
            .unwrap()
            .starts_with("127.0.0.1:")
    );
    assert_eq!(body["keys"][0]["name"], "ci");
    assert_eq!(body["keys"][0]["repository"], "github.com/acme/web");
    assert!(body["keys"][0]["createdAt"].is_string());
    assert!(body["trusts"].as_array().unwrap().is_empty());
    let text = body.to_string();
    assert!(!text.contains(&secret), "a key secret must never be listed");
    assert!(!text.contains("sha256"));

    let bare = start(false, ServerOptions::default()).await;
    let body = bare.ok("/api/v1/settings").await;
    assert_eq!(body["server"]["publishRequiresAuth"], false);
    assert_eq!(body["server"]["demo"], false);
}

#[tokio::test]
async fn a_new_console_answers_every_read_endpoint_with_empty_lists() {
    let console = start(false, ServerOptions::default()).await;
    let projects = console.ok("/api/v1/projects").await;
    assert_eq!(projects["projects"].as_array().unwrap().len(), 0);
    assert_eq!(projects["summary"]["projects"], 0);
    assert!(projects["summary"]["lastReportAt"].is_null());
    assert!(
        console.ok("/api/v1/capabilities").await["capabilities"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        console.ok("/api/v1/harnesses").await["harnesses"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        console.ok("/api/v1/policies").await["gaps"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        console.ok("/api/v1/events").await["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn the_ui_is_served_from_the_binary_with_a_strict_content_policy() {
    let console = start(false, ServerOptions::default()).await;
    for (path, content_type) in [
        ("/", "text/html"),
        ("/assets/app.js", "text/javascript"),
        ("/assets/app.css", "text/css"),
    ] {
        let response = console
            .client
            .get(format!("{}{path}", console.base))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "{path}");
        let headers = response.headers().clone();
        assert!(
            headers["content-type"]
                .to_str()
                .unwrap()
                .starts_with(content_type),
            "{path}"
        );
        assert_eq!(headers["x-content-type-options"], "nosniff");
        let policy = headers["content-security-policy"].to_str().unwrap();
        assert!(policy.contains("default-src 'none'"), "{path}: {policy}");
        assert!(!policy.contains("unsafe-inline"), "{path}: {policy}");
        let body = response.text().await.unwrap();
        assert!(!body.is_empty());
    }
    let page = console
        .client
        .get(format!("{}/", console.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains("/assets/app.js") && page.contains("/assets/app.css"));

    let api = console
        .client
        .get(format!("{}/api/v1/projects", console.base))
        .send()
        .await
        .unwrap();
    assert_eq!(api.headers()["x-content-type-options"], "nosniff");
    assert!(api.headers().get("content-security-policy").is_none());
}

#[test]
fn the_ui_files_load_nothing_from_the_network() {
    for (name, source) in [
        ("index.html", include_str!("../ui/index.html")),
        ("app.css", include_str!("../ui/app.css")),
        ("app.js", include_str!("../ui/app.js")),
    ] {
        // app.js names one external address, the documentation link a
        // visitor can click, and never fetches it. The page icon is an
        // inline SVG, whose namespace is an identifier and not a request.
        let text = source
            .replace("https://tuffcli.dev/cli/console/", "")
            .replace("http://www.w3.org/2000/svg", "");
        for needle in [
            "http://", "https://", "//fonts.", "@import", "<style", "style=\"",
        ] {
            assert!(
                !text.contains(needle),
                "{name} contains {needle}, which the content security policy or the no-network rule forbids"
            );
        }
    }
    assert!(!include_str!("../ui/index.html").contains("googleapis"));
}
