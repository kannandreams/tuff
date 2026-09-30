//! The Tuff console server (RFC-108): stores the reports that
//! `tuff console publish` sends and serves them over HTTP.
//!
//! [`store`] owns the SQLite file: schema and migrations, report ingest with
//! deduplication, and publish keys. [`server`] owns the HTTP API and the
//! rules for which address the server may bind. The `tuff` binary wires both
//! to `tuff console serve` and `tuff console key`.

pub mod server;
pub mod store;

pub use server::{ServeConfig, check_bind, router, run, serve};
pub use store::{IngestOutcome, KeyInfo, ProjectRow, Store, default_data_dir};
