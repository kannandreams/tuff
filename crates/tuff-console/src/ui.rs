//! The console's web UI: three static files compiled into the binary. They
//! call the JSON API under `/api/v1` and load nothing from elsewhere.

use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};

const INDEX: &str = include_str!("../ui/index.html");
const STYLESHEET: &str = include_str!("../ui/app.css");
const SCRIPT: &str = include_str!("../ui/app.js");

fn file(content_type: &'static str, body: &'static str) -> Response {
    let mut response = body.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    // The files change with the binary, so a browser checks them each time.
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

pub async fn index() -> Response {
    file("text/html; charset=utf-8", INDEX)
}

pub async fn stylesheet() -> Response {
    file("text/css; charset=utf-8", STYLESHEET)
}

pub async fn script() -> Response {
    file("text/javascript; charset=utf-8", SCRIPT)
}
