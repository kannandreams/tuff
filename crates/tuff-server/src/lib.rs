//! The Tuff dashboard server (RFC-108): stores the reports that
//! `tuff dashboard publish` sends and serves them over HTTP.
//!
//! [`store`] owns the SQLite file: schema and migrations, report ingest with
//! deduplication, and publish tokens. [`server`] owns the HTTP API and the
//! rules for which address the server may bind. The `tuff` binary wires both
//! to `tuff dashboard serve` and `tuff dashboard token`.

pub mod server;
pub mod store;

pub use server::{ServeConfig, check_bind, router, run, serve};
pub use store::{IngestOutcome, ProjectRow, Store, TokenInfo, default_data_dir};
