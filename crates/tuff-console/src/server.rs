//! The console's HTTP API (RFC-108 D5 and D9) and the rules for where it
//! may listen.

use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use tuff_core::error::{ErrorKind, Result, TuffError};
use tuff_core::report::{REPORT_SCHEMA, Report, normalize_remote};

use crate::oidc::{OidcError, Trust, Verifier};
use crate::store::{EventFilter, KeyGrant, Store};
use crate::ui;
use crate::views::{self, ProjectModel};

/// Largest report body the server reads.
const MAX_REPORT_BYTES: usize = 16 * 1024 * 1024;

/// Where and how `tuff console serve` listens.
#[derive(Debug, Clone)]
pub struct ServeConfig {
    pub data_dir: PathBuf,
    pub addr: SocketAddr,
    /// Acknowledges that viewing is not authenticated on a non-loopback
    /// address (D5).
    pub public_read: bool,
    /// Sources of OIDC tokens the console accepts for publishing.
    pub trusts: Vec<Trust>,
    /// The URL publishers reach the console at, which OIDC tokens name as
    /// their audience. `http://<bound address>` when unset.
    pub public_url: Option<String>,
    /// Serve generated sample projects from a temporary database instead of
    /// the data folder.
    pub demo: bool,
}

/// How the running server decides who may publish, and what the UI and
/// `GET /api/v1/settings` tell viewers about it.
#[derive(Clone, Default)]
pub struct ServerOptions {
    /// Publishing always needs a credential, even with no key created yet.
    pub require_key: bool,
    /// Verifies OIDC tokens when a trust is configured.
    pub oidc: Option<Arc<Verifier>>,
    /// The address the server listens on. [`serve`] fills it from the
    /// listener when it is unset.
    pub address: Option<SocketAddr>,
    /// Whether the server was started with `--public-read`.
    pub public_read: bool,
    /// Whether the data is generated sample data.
    pub demo: bool,
}

/// The bind rules of D5. A loopback address needs nothing. Any other
/// address needs `--public-read`, because viewing is not authenticated, and
/// at least one publish credential (a key or a trust), because publishing
/// is authenticated. A demo needs no credential: its data is generated and
/// in memory, and with none configured every publish is refused, so it is
/// read only.
pub fn check_bind(
    addr: SocketAddr,
    public_read: bool,
    credential_count: u64,
    demo: bool,
) -> Result<()> {
    if addr.ip().is_loopback() {
        return Ok(());
    }
    if !public_read {
        return Err(TuffError::refused(format!(
            "{addr} is not a loopback address, and the console does not authenticate people who view it"
        ))
        .with_hint(
            "put the server behind a reverse proxy that authenticates viewers and pass --public-read, or bind 127.0.0.1",
        ));
    }
    if credential_count == 0 && !demo {
        return Err(TuffError::refused(format!(
            "{addr} is not a loopback address, and no publish key or trust exists"
        ))
        .with_hint("run 'tuff console key create <name>' first, or pass --trust github:<owner>"));
    }
    Ok(())
}

/// Open the store, apply the bind rules, and serve until interrupted.
/// `on_ready` receives the bound address once the server accepts
/// connections.
pub fn run(config: ServeConfig, on_ready: impl FnOnce(SocketAddr)) -> Result<()> {
    let store = if config.demo {
        let store = Store::open_in_memory()?;
        crate::demo::populate(&store)?;
        Arc::new(store)
    } else {
        Arc::new(Store::open(&config.data_dir)?)
    };
    check_bind(
        config.addr,
        config.public_read,
        store.key_count()? + config.trusts.len() as u64,
        config.demo,
    )?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind(config.addr)
            .await
            .map_err(|error| {
                TuffError::of(
                    ErrorKind::Io,
                    format!("cannot listen on {}: {error}", config.addr),
                )
                .with_hint("pass --addr with a free address, for example 127.0.0.1:7475")
            })?;
        let bound = listener.local_addr()?;
        let public_url = config
            .public_url
            .clone()
            .unwrap_or_else(|| format!("http://{bound}"));
        let oidc = if config.trusts.is_empty() {
            None
        } else {
            Some(Arc::new(Verifier::new(config.trusts.clone(), &public_url)?))
        };
        on_ready(bound);
        serve(
            store,
            listener,
            ServerOptions {
                require_key: !config.addr.ip().is_loopback(),
                oidc,
                address: Some(bound),
                public_read: config.public_read,
                demo: config.demo,
            },
            shutdown_signal(),
        )
        .await
    })
}

async fn shutdown_signal() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        } else {
            std::future::pending::<()>().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = interrupt => {},
        () = terminate => {},
    }
}

/// Serve `router(store, options)` on `listener` until `shutdown` completes.
pub async fn serve(
    store: Arc<Store>,
    listener: tokio::net::TcpListener,
    mut options: ServerOptions,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<()> {
    if options.address.is_none() {
        options.address = listener.local_addr().ok();
    }
    axum::serve(listener, router(store, options))
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

#[derive(Clone)]
struct AppState {
    store: Arc<Store>,
    options: ServerOptions,
}

/// The API routes. Publishing needs a live key or a verified OIDC token in
/// `Authorization: Bearer` when `require_key` is set, when a trust is
/// configured, and whenever at least one key exists, on any address. With
/// none of those, anyone who can connect may publish.
pub fn router(store: Arc<Store>, options: ServerOptions) -> Router {
    let state = AppState { store, options };
    Router::new()
        .route("/", get(ui::index))
        .route("/assets/app.css", get(ui::stylesheet))
        .route("/assets/app.js", get(ui::script))
        .route("/healthz", get(healthz))
        .route("/api/v1/healthz", get(healthz))
        .route("/api/v1/reports", post(post_report))
        .route("/api/v1/projects", get(list_projects))
        .route("/api/v1/projects/{id}", get(get_project))
        .route("/api/v1/projects/{id}/reports", get(project_reports))
        .route("/api/v1/capabilities", get(list_capabilities))
        .route("/api/v1/capabilities/{type}/{*id}", get(get_capability))
        .route("/api/v1/harnesses", get(get_harnesses))
        .route("/api/v1/policies", get(get_policies))
        .route("/api/v1/events", get(list_events))
        .route("/api/v1/settings", get(get_settings))
        .layer(DefaultBodyLimit::max(MAX_REPORT_BYTES))
        .layer(axum::middleware::from_fn(security_headers))
        .with_state(state)
}

/// An error rendered in the CLI's `--json` envelope.
struct ApiError {
    status: StatusCode,
    kind: &'static str,
    message: String,
    hint: Option<String>,
}

impl ApiError {
    fn new(status: StatusCode, kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            kind,
            message: message.into(),
            hint: None,
        }
    }

    fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl From<TuffError> for ApiError {
    fn from(error: TuffError) -> Self {
        let status = match error.kind() {
            ErrorKind::Usage => StatusCode::BAD_REQUEST,
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let mut api = Self::new(status, error.kind().as_str(), error.message());
        api.hint = error.hint().map(str::to_string);
        api
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut body = json!({ "error": { "kind": self.kind, "message": self.message } });
        if let Some(hint) = self.hint {
            body["error"]["hint"] = hint.into();
        }
        let mut response = (self.status, Json(body)).into_response();
        if self.status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                header::HeaderValue::from_static("Bearer"),
            );
        }
        response
    }
}

/// Run blocking store work off the async threads.
async fn blocking<T: Send + 'static>(
    store: &Arc<Store>,
    work: impl FnOnce(&Store) -> Result<T> + Send + 'static,
) -> std::result::Result<T, ApiError> {
    let store = Arc::clone(store);
    tokio::task::spawn_blocking(move || work(&store))
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                format!("store task failed: {error}"),
            )
        })?
        .map_err(ApiError::from)
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

/// Who is publishing.
enum Principal {
    /// Nothing is configured, so nobody is asked.
    Anonymous,
    Key(KeyGrant),
    /// A verified token, bound to the repository it names.
    Oidc {
        report_repository: String,
    },
}

fn unauthorized(message: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized", message)
}

async fn authorize(
    state: &AppState,
    headers: &HeaderMap,
) -> std::result::Result<Principal, ApiError> {
    let configured = state.options.require_key
        || state.options.oidc.is_some()
        || blocking(&state.store, Store::key_count).await? > 0;
    if !configured {
        return Ok(Principal::Anonymous);
    }
    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|token| !token.is_empty());
    let Some(token) = presented else {
        return Err(unauthorized("publishing to this console needs a credential")
            .hint("send 'Authorization: Bearer <key>', set TUFF_CONSOLE_KEY for 'tuff console publish', or publish from a trusted GitHub Actions job"));
    };

    let verifier = match state.options.oidc.as_ref() {
        Some(verifier) if !token.starts_with(crate::store::KEY_PREFIX) => verifier,
        _ => {
            let key = token.to_string();
            return match blocking(&state.store, move |store| store.verify_key(&key)).await? {
                Some(grant) => Ok(Principal::Key(grant)),
                None => Err(unauthorized("the key is not valid or was revoked")
                    .hint("create one with 'tuff console key create <name>' on the server")),
            };
        }
    };
    match verifier.verify(token).await {
        Ok(verified) => Ok(Principal::Oidc {
            report_repository: verified.report_repository,
        }),
        Err(OidcError::Invalid(reason)) => Err(unauthorized(reason).hint(format!(
            "the token's audience must be {}, and the job needs 'permissions: id-token: write'",
            verifier.audience()
        ))),
        Err(OidcError::Untrusted(reason)) => {
            Err(ApiError::new(StatusCode::FORBIDDEN, "refused", reason)
                .hint("start the console with --trust github:<owner> for this owner"))
        }
        Err(OidcError::Unavailable(reason)) => {
            Err(
                ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "source_failed", reason)
                    .hint("the console must reach the token issuer; retry later"),
            )
        }
    }
}

/// A scoped credential publishes only for its own repository (D5).
fn check_binding(principal: &Principal, repository: &str) -> std::result::Result<(), ApiError> {
    let bound = match principal {
        Principal::Anonymous
        | Principal::Key(KeyGrant {
            repository: None, ..
        }) => return Ok(()),
        Principal::Key(KeyGrant {
            repository: Some(bound),
            ..
        })
        | Principal::Oidc {
            report_repository: bound,
        } => bound,
    };
    let reported = normalize_remote(repository);
    if normalize_remote(bound).eq_ignore_ascii_case(&reported) {
        return Ok(());
    }
    Err(ApiError::new(
        StatusCode::FORBIDDEN,
        "refused",
        format!("this credential may publish only for {bound}, and the report is for {reported}"),
    )
    .hint("publish each repository with its own credential"))
}

async fn post_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> std::result::Result<Response, ApiError> {
    let principal = authorize(&state, &headers).await?;

    let raw: serde_json::Value = serde_json::from_slice(&body).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "usage",
            format!("the body is not JSON: {error}"),
        )
    })?;
    match raw.get("schema").and_then(serde_json::Value::as_u64) {
        Some(schema) if schema == u64::from(REPORT_SCHEMA) => {}
        other => {
            let seen = other.map_or_else(|| "none".to_string(), |schema| schema.to_string());
            return Err(ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "unsupported",
                format!(
                    "report schema {seen} is not supported, and this server reads schema {REPORT_SCHEMA}"
                ),
            )
            .hint("use a tuff version that matches the server's"));
        }
    }
    let report: Report = serde_json::from_value(raw.clone()).map_err(|error| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "corrupt",
            format!("the report is not valid: {error}"),
        )
    })?;
    if report.project.repository.trim().is_empty() || report.project.path.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "corrupt",
            "the report's project needs a repository and a path",
        ));
    }

    check_binding(&principal, &report.project.repository)?;

    let outcome = blocking(&state.store, move |store| store.ingest(&report, &raw)).await?;
    let status = if outcome.deduplicated {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, Json(outcome)).into_response())
}

type ApiResult = std::result::Result<Json<serde_json::Value>, ApiError>;

/// The latest report of every project, read into the shapes the views use.
async fn models(state: &AppState) -> std::result::Result<Vec<ProjectModel>, ApiError> {
    blocking(&state.store, |store| {
        Ok(store
            .latest_reports()?
            .into_iter()
            .map(|(row, body)| ProjectModel::new(row, &body))
            .collect())
    })
    .await
}

async fn list_projects(State(state): State<AppState>) -> ApiResult {
    Ok(Json(views::projects(&models(&state).await?)))
}

fn no_project(id: i64) -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "not_found",
        format!("no project {id}"),
    )
    .hint("GET /api/v1/projects lists the ids")
}

async fn get_project(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult {
    match blocking(&state.store, move |store| store.project(id)).await? {
        Some((row, latest_report)) => Ok(Json(views::project(
            &ProjectModel::new(row, &latest_report),
            latest_report,
        ))),
        None => Err(no_project(id)),
    }
}

async fn project_reports(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult {
    let reports = blocking(&state.store, move |store| {
        if store.project(id)?.is_none() {
            return Ok(None);
        }
        Ok(Some(store.report_history(id, 500)?))
    })
    .await?;
    match reports {
        Some(reports) => Ok(Json(json!({ "projectId": id, "reports": reports }))),
        None => Err(no_project(id)),
    }
}

async fn list_capabilities(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult {
    let models = models(&state).await?;
    Ok(Json(views::capabilities(
        &models,
        query
            .get("type")
            .map(String::as_str)
            .filter(|t| !t.is_empty()),
    )))
}

async fn get_capability(
    State(state): State<AppState>,
    Path((capability_type, id)): Path<(String, String)>,
) -> ApiResult {
    let models = models(&state).await?;
    match views::capability(&models, &capability_type, &id) {
        Some(value) => Ok(Json(value)),
        None => Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("no project uses {capability_type} '{id}'"),
        )
        .hint("GET /api/v1/capabilities lists what is in use")),
    }
}

async fn get_harnesses(State(state): State<AppState>) -> ApiResult {
    Ok(Json(views::harnesses(&models(&state).await?)))
}

async fn get_policies(State(state): State<AppState>) -> ApiResult {
    Ok(Json(views::policies(&models(&state).await?)))
}

/// The most events one request returns.
const MAX_EVENTS: u32 = 1000;

async fn list_events(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult {
    let text = |key: &str| query.get(key).filter(|value| !value.is_empty()).cloned();
    let usage = |message: String| {
        ApiError::new(StatusCode::BAD_REQUEST, "usage", message).hint(
            "filters are project (an id), capability, kind, since (a date or time), before (an event id), and limit",
        )
    };
    let project_id = match text("project") {
        Some(value) => Some(
            value
                .parse::<i64>()
                .map_err(|_| usage(format!("project '{value}' is not a project id")))?,
        ),
        None => None,
    };
    let limit = match text("limit") {
        Some(value) => value
            .parse::<u32>()
            .map_err(|_| usage(format!("limit '{value}' is not a number")))?
            .clamp(1, MAX_EVENTS),
        None => 200,
    };
    let before = match text("before") {
        Some(value) => Some(
            value
                .parse::<i64>()
                .map_err(|_| usage(format!("before '{value}' is not an event id")))?,
        ),
        None => None,
    };
    let kind = text("kind");
    if let Some(kind) = &kind
        && !crate::events::kind::ALL.contains(&kind.as_str())
    {
        return Err(usage(format!("'{kind}' is not an event kind")));
    }
    let since = text("since");
    if let Some(since) = &since
        && !since.starts_with(|c: char| c.is_ascii_digit())
    {
        return Err(usage(format!("since '{since}' is not a date or time")));
    }
    let filter = EventFilter {
        project_id,
        capability: text("capability"),
        kind,
        since,
        before,
        limit: Some(limit),
    };
    let (events, projects) = blocking(&state.store, move |store| {
        Ok((store.events(&filter)?, store.projects()?))
    })
    .await?;
    let events: Vec<serde_json::Value> = events
        .into_iter()
        .map(|event| {
            let mut value = serde_json::to_value(&event).unwrap_or_default();
            if let Some(project) = projects.iter().find(|p| p.id == event.project_id) {
                value["projectName"] = json!(project.name);
                value["repository"] = json!(project.repository);
                value["path"] = json!(project.path);
            }
            value
        })
        .collect();
    // A full page may have more behind it: the id to pass as `before`.
    let next_before = (events.len() == limit as usize)
        .then(|| events.last().and_then(|event| event["id"].as_i64()))
        .flatten();
    Ok(Json(
        json!({ "events": events, "kinds": crate::events::kind::ALL, "nextBefore": next_before }),
    ))
}

async fn get_settings(State(state): State<AppState>) -> ApiResult {
    let keys = blocking(&state.store, Store::keys).await?;
    let options = &state.options;
    let trusts: Vec<serde_json::Value> = options
        .oidc
        .iter()
        .flat_map(|verifier| verifier.trusts())
        .map(|trust| json!({ "provider": trust.provider, "owner": trust.owner }))
        .collect();
    let requires_auth = options.require_key || options.oidc.is_some() || !keys.is_empty();
    let loopback = options
        .address
        .is_none_or(|address| address.ip().is_loopback());
    Ok(Json(json!({
        "server": {
            "version": env!("CARGO_PKG_VERSION"),
            "address": options.address.map(|address| address.to_string()),
            "loopback": loopback,
            "publicRead": options.public_read,
            "publishRequiresAuth": requires_auth,
            "demo": options.demo,
            "audience": options.oidc.as_ref().map(|verifier| verifier.audience()),
        },
        "trusts": trusts,
        "keys": keys,
    })))
}

/// Headers for every response. The UI is static files that load nothing
/// from elsewhere, and the content security policy says so to the browser.
async fn security_headers(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let path = request.uri().path().to_string();
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        header::HeaderValue::from_static("no-referrer"),
    );
    if !path.starts_with("/api/") && path != "/healthz" {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            header::HeaderValue::from_static(
                "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
            ),
        );
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(text: &str) -> SocketAddr {
        text.parse().unwrap()
    }

    #[test]
    fn loopback_binds_need_nothing() {
        for text in ["127.0.0.1:7474", "127.0.0.1:0", "[::1]:7474"] {
            check_bind(addr(text), false, 0, false).unwrap();
        }
    }

    #[test]
    fn a_public_bind_needs_public_read() {
        for text in ["0.0.0.0:7474", "192.168.1.20:7474", "[::]:7474"] {
            let error = check_bind(addr(text), false, 1, false).unwrap_err();
            assert_eq!(error.kind(), ErrorKind::Refused, "{text}");
            assert!(error.hint().unwrap().contains("--public-read"));
        }
    }

    #[test]
    fn a_public_bind_needs_a_key() {
        let error = check_bind(addr("0.0.0.0:7474"), true, 0, false).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Refused);
        assert!(error.hint().unwrap().contains("key create"));
        check_bind(addr("0.0.0.0:7474"), true, 1, false).unwrap();
    }

    #[test]
    fn a_public_demo_needs_public_read_but_no_key() {
        check_bind(addr("0.0.0.0:7474"), true, 0, true).unwrap();
        let error = check_bind(addr("0.0.0.0:7474"), false, 0, true).unwrap_err();
        assert!(error.hint().unwrap().contains("--public-read"));
    }
}
