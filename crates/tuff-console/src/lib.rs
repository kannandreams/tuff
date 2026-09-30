//! The Tuff console server (RFC-108): stores the reports that
//! `tuff console publish` sends and serves them over HTTP.
//!
//! [`store`] owns the SQLite file: schema and migrations, report ingest with
//! deduplication, the audit events and inventory computed on ingest, and
//! publish keys. [`oidc`] verifies the GitHub Actions tokens that publish
//! without a secret. [`server`] owns the HTTP API and the rules for which
//! address the server may bind. The `tuff` binary wires them to
//! `tuff console serve` and `tuff console key`.

pub mod events;
pub mod oidc;
pub mod server;
pub mod store;

pub use oidc::{Trust, Verifier};
pub use server::{ServeConfig, ServerOptions, check_bind, router, run, serve};
pub use store::{
    EventFilter, EventRow, IngestOutcome, KeyGrant, KeyInfo, ProjectRow, Store, default_data_dir,
};
