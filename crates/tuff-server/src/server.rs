//! The dashboard's HTTP API (RFC-108 D5 and D9) and the rules for where it
//! may listen.

use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use tuff_core::error::{ErrorKind, Result, TuffError};
use tuff_core::report::{REPORT_SCHEMA, Report};

use crate::store::Store;

/// Largest report body the server reads.
const MAX_REPORT_BYTES: usize = 16 * 1024 * 1024;

/// Where and how `tuff dashboard serve` listens.
#[derive(Debug, Clone)]
pub struct ServeConfig {
    pub data_dir: PathBuf,
    pub addr: SocketAddr,
    /// Acknowledges that viewing is not authenticated on a non-loopback
    /// address (D5).
    pub public_read: bool,
}

/// The bind rules of D5. A loopback address needs nothing. Any other
/// address needs `--public-read`, because viewing is not authenticated, and
/// at least one publish token, because publishing is.
pub fn check_bind(addr: SocketAddr, public_read: bool, token_count: u64) -> Result<()> {
    if addr.ip().is_loopback() {
        return Ok(());
    }
    if !public_read {
        return Err(TuffError::refused(format!(
            "{addr} is not a loopback address, and the dashboard does not authenticate people who view it"
        ))
        .with_hint(
            "put the server behind a reverse proxy that authenticates viewers and pass --public-read, or bind 127.0.0.1",
        ));
    }
    if token_count == 0 {
        return Err(TuffError::refused(format!(
            "{addr} is not a loopback address, and no publish token exists"
        ))
        .with_hint("run 'tuff dashboard token create <name>' first"));
    }
    Ok(())
}

/// Open the store, apply the bind rules, and serve until interrupted.
/// `on_ready` receives the bound address once the server accepts
/// connections.
pub fn run(config: ServeConfig, on_ready: impl FnOnce(SocketAddr)) -> Result<()> {
    let store = Arc::new(Store::open(&config.data_dir)?);
    check_bind(config.addr, config.public_read, store.token_count()?)?;
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
        on_ready(listener.local_addr()?);
        serve(
            store,
            listener,
            !config.addr.ip().is_loopback(),
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

/// Serve `router(store, require_token)` on `listener` until `shutdown`
/// completes.
pub async fn serve(
    store: Arc<Store>,
    listener: tokio::net::TcpListener,
    require_token: bool,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<()> {
    axum::serve(listener, router(store, require_token))
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

#[derive(Clone)]
struct AppState {
    store: Arc<Store>,
    require_token: bool,
}

/// The API routes. With `require_token`, publishing needs a live token in
/// `Authorization: Bearer`; without it, anyone who can connect may publish.
pub fn router(store: Arc<Store>, require_token: bool) -> Router {
    let state = AppState {
        store,
        require_token,
    };
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/v1/healthz", get(healthz))
        .route("/api/v1/reports", post(post_report))
        .route("/api/v1/projects", get(list_projects))
        .route("/api/v1/projects/{id}", get(get_project))
        .layer(DefaultBodyLimit::max(MAX_REPORT_BYTES))
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

async fn authorize(state: &AppState, headers: &HeaderMap) -> std::result::Result<(), ApiError> {
    if !state.require_token {
        return Ok(());
    }
    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|token| !token.is_empty());
    let Some(token) = presented else {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "publishing to this dashboard needs a token",
        )
        .hint("send 'Authorization: Bearer <token>', or set TUFF_DASHBOARD_TOKEN for 'tuff dashboard publish'"));
    };
    let token = token.to_string();
    if blocking(&state.store, move |store| store.verify_token(&token)).await? {
        Ok(())
    } else {
        Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "the token is not valid or was revoked",
        )
        .hint("create one with 'tuff dashboard token create <name>' on the server"))
    }
}

async fn post_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> std::result::Result<Response, ApiError> {
    authorize(&state, &headers).await?;

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

    let outcome = blocking(&state.store, move |store| store.ingest(&report, &raw)).await?;
    let status = if outcome.deduplicated {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, Json(outcome)).into_response())
}

async fn list_projects(
    State(state): State<AppState>,
) -> std::result::Result<Json<serde_json::Value>, ApiError> {
    let projects = blocking(&state.store, Store::projects).await?;
    Ok(Json(json!({ "projects": projects })))
}

async fn get_project(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> std::result::Result<Json<serde_json::Value>, ApiError> {
    match blocking(&state.store, move |store| store.project(id)).await? {
        Some((project, latest_report)) => Ok(Json(
            json!({ "project": project, "latestReport": latest_report }),
        )),
        None => Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("no project {id}"),
        )
        .hint("GET /api/v1/projects lists the ids")),
    }
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
            check_bind(addr(text), false, 0).unwrap();
        }
    }

    #[test]
    fn a_public_bind_needs_public_read() {
        for text in ["0.0.0.0:7474", "192.168.1.20:7474", "[::]:7474"] {
            let error = check_bind(addr(text), false, 1).unwrap_err();
            assert_eq!(error.kind(), ErrorKind::Refused, "{text}");
            assert!(error.hint().unwrap().contains("--public-read"));
        }
    }

    #[test]
    fn a_public_bind_needs_a_token() {
        let error = check_bind(addr("0.0.0.0:7474"), true, 0).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Refused);
        assert!(error.hint().unwrap().contains("token create"));
        check_bind(addr("0.0.0.0:7474"), true, 1).unwrap();
    }
}
