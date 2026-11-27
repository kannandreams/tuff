//! Tuff capability packs from Rust: build a source pack for its harnesses
//! and bundle formats, describe a pack for a review screen, read a Claude
//! Code plugin as a pack, check out a pack from git and compare it with its
//! release tags, preview an update, and write packs as a Claude Code
//! marketplace.
//!
//! The `tuff` command is built on these functions, so they behave exactly
//! as `tuff pack` and `tuff update --check` do. They return values and
//! never print: notes and warnings come back on the result for the caller
//! to show. They write only to the folders they are given and to temporary
//! folders, from the system's temporary directory, that they remove. They
//! read no environment variables of their own, so they need no `HOME`. git
//! runs as a program and reads the machine's git configuration and
//! credentials.
//!
//! Installing, updating, and removing packs is not in this crate yet; until
//! it is, run `tuff add pack`, `tuff update`, and `tuff pack remove` with
//! `--json`. The JSON documents are specified at
//! <https://tuffcli.dev/spec/packs/>.
//!
//! The Rust API is 0.x: it can change in a minor release, and each change is
//! in the changelog.

pub mod adapter;
pub mod build;
pub mod git;
pub mod import;
pub mod inspect;
pub mod marketplace;
pub mod preview;

pub use tuff_core::error::{ErrorKind, Result, TuffError};
