//! The console's SQLite file (RFC-108 D6).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tuff_core::error::{ErrorKind, Result, TuffError};
use tuff_core::report::Report;

/// File name of the database inside the data directory.
pub const DATABASE_FILE: &str = "console.sqlite";

/// Prefix of every publish key, so a leaked one is recognisable.
pub const KEY_PREFIX: &str = "tuffc_";

/// Each entry moves the schema from version `index` to `index + 1`, tracked
/// in `PRAGMA user_version`. Entries are never edited once released.
const MIGRATIONS: &[&str] = &[
    // 1: the D6 tables. `inventory` and `events` are filled by later
    // milestones (the audit trail and the read API) and exist from the start
    // so the first release of the file format holds the whole schema.
    "
    CREATE TABLE projects (
        id INTEGER PRIMARY KEY,
        repository TEXT NOT NULL,
        path TEXT NOT NULL,
        name TEXT NOT NULL,
        first_report_at TEXT NOT NULL,
        last_report_at TEXT NOT NULL,
        UNIQUE (repository, path)
    );
    CREATE TABLE reports (
        id INTEGER PRIMARY KEY,
        project_id INTEGER NOT NULL REFERENCES projects (id),
        received_at TEXT NOT NULL,
        generated_at TEXT NOT NULL,
        commit_sha TEXT,
        branch TEXT,
        tuff_version TEXT NOT NULL,
        digest TEXT NOT NULL,
        body TEXT NOT NULL
    );
    CREATE INDEX reports_by_project ON reports (project_id, id);
    CREATE TABLE inventory (
        project_id INTEGER NOT NULL REFERENCES projects (id),
        capability_type TEXT NOT NULL,
        capability_id TEXT NOT NULL,
        target TEXT NOT NULL,
        version TEXT,
        source TEXT,
        status TEXT NOT NULL,
        PRIMARY KEY (project_id, capability_type, capability_id, target)
    );
    CREATE TABLE events (
        id INTEGER PRIMARY KEY,
        project_id INTEGER NOT NULL REFERENCES projects (id),
        report_id INTEGER NOT NULL REFERENCES reports (id),
        kind TEXT NOT NULL,
        capability_type TEXT,
        capability_id TEXT,
        target TEXT,
        detail TEXT,
        commit_sha TEXT,
        occurred_at TEXT NOT NULL
    );
    CREATE INDEX events_by_project ON events (project_id, id);
    CREATE TABLE keys (
        name TEXT PRIMARY KEY,
        sha256 TEXT NOT NULL UNIQUE,
        created_at TEXT NOT NULL,
        last_used_at TEXT
    );
    ",
];

/// The data directory when `--data` is not given:
/// `$XDG_DATA_HOME/tuff/console`, or `~/.local/share/tuff/console`.
pub fn default_data_dir(home: &Path) -> PathBuf {
    tuff_core::paths::user_data(home).join("console")
}

/// What ingesting a report did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestOutcome {
    pub project_id: i64,
    /// The stored report, or the previous one when `deduplicated`.
    pub report_id: i64,
    /// The report equalled the project's previous one, so only the
    /// project's last report time changed.
    pub deduplicated: bool,
    pub project_first_seen: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRow {
    pub id: i64,
    pub repository: String,
    pub path: String,
    pub name: String,
    pub first_report_at: String,
    pub last_report_at: String,
    pub report_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyInfo {
    pub name: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

pub struct Store {
    conn: Mutex<Connection>,
}

fn db_error(error: rusqlite::Error) -> TuffError {
    TuffError::of(ErrorKind::Source, format!("console database: {error}")).with_source(error)
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// JSON with object keys in sorted order and no whitespace, so equal
/// documents produce equal bytes whatever order their keys arrived in.
fn write_canonical(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::Value::String(key.clone()).to_string());
                out.push(':');
                write_canonical(&map[key], out);
            }
            out.push('}');
        }
        serde_json::Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// SHA-256 over the report's canonical JSON without `generatedAt` (D6).
pub fn report_digest(report: &serde_json::Value) -> String {
    let mut value = report.clone();
    if let Some(map) = value.as_object_mut() {
        map.remove("generatedAt");
    }
    let mut canonical = String::new();
    write_canonical(&value, &mut canonical);
    sha256_hex(canonical.as_bytes())
}

fn valid_key_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

impl Store {
    /// Open, creating the directory and file when missing, and bring the
    /// schema up to date.
    pub fn open(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir).map_err(|error| {
            TuffError::of(
                ErrorKind::Io,
                format!("cannot create {}: {error}", data_dir.display()),
            )
            .with_hint("pass --data <dir> with a folder you can write to")
        })?;
        let path = data_dir.join(DATABASE_FILE);
        let created = !path.exists();
        let conn = Connection::open(&path).map_err(|error| {
            TuffError::of(
                ErrorKind::Source,
                format!("cannot open {}: {error}", path.display()),
            )
            .with_hint("pass --data <dir> with a folder you can write to")
        })?;
        #[cfg(unix)]
        if created {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        #[cfg(not(unix))]
        let _ = created;
        Self::from_connection(conn, &path.display().to_string())
    }

    /// An in-memory store, for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory().map_err(db_error)?, ":memory:")
    }

    fn from_connection(mut conn: Connection, label: &str) -> Result<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(db_error)?;
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(db_error)?;
        // Not every database can change mode, such as one in memory.
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        migrate(&mut conn, label)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        // A panic while holding the lock leaves the connection usable.
        self.conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The schema version of the file.
    pub fn schema_version(&self) -> Result<u32> {
        self.conn()
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(db_error)
    }

    /// Store one report. `raw` is the report as received, kept verbatim in
    /// the `reports` row. A report equal to the project's previous one
    /// (digest over everything except `generatedAt`) adds no row and moves
    /// the project's last report time.
    pub fn ingest(&self, report: &Report, raw: &serde_json::Value) -> Result<IngestOutcome> {
        let digest = report_digest(raw);
        let body = serde_json::to_string(raw)?;
        let received_at = now();
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;

        let project: Option<i64> = tx
            .query_row(
                "SELECT id FROM projects WHERE repository = ?1 AND path = ?2",
                params![report.project.repository, report.project.path],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_error)?;
        let (project_id, project_first_seen) = match project {
            Some(id) => (id, false),
            None => {
                tx.execute(
                    "INSERT INTO projects (repository, path, name, first_report_at, last_report_at)
                     VALUES (?1, ?2, ?3, ?4, ?4)",
                    params![
                        report.project.repository,
                        report.project.path,
                        report.project.name,
                        received_at
                    ],
                )
                .map_err(db_error)?;
                (tx.last_insert_rowid(), true)
            }
        };

        let previous: Option<(i64, String)> = tx
            .query_row(
                "SELECT id, digest FROM reports WHERE project_id = ?1 ORDER BY id DESC LIMIT 1",
                params![project_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(db_error)?;

        let outcome = match previous {
            Some((report_id, previous_digest)) if previous_digest == digest => {
                tx.execute(
                    "UPDATE projects SET last_report_at = ?2 WHERE id = ?1",
                    params![project_id, received_at],
                )
                .map_err(db_error)?;
                IngestOutcome {
                    project_id,
                    report_id,
                    deduplicated: true,
                    project_first_seen,
                }
            }
            _ => {
                tx.execute(
                    "INSERT INTO reports (project_id, received_at, generated_at, commit_sha, branch,
                                          tuff_version, digest, body)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        project_id,
                        received_at,
                        report.generated_at,
                        report.project.commit,
                        report.project.branch,
                        report.tuff_version,
                        digest,
                        body
                    ],
                )
                .map_err(db_error)?;
                let report_id = tx.last_insert_rowid();
                tx.execute(
                    "UPDATE projects SET name = ?2, last_report_at = ?3 WHERE id = ?1",
                    params![project_id, report.project.name, received_at],
                )
                .map_err(db_error)?;
                IngestOutcome {
                    project_id,
                    report_id,
                    deduplicated: false,
                    project_first_seen,
                }
            }
        };
        tx.commit().map_err(db_error)?;
        Ok(outcome)
    }

    /// Every project, in the order they were first seen.
    pub fn projects(&self) -> Result<Vec<ProjectRow>> {
        let conn = self.conn();
        let mut statement = conn
            .prepare(
                "SELECT p.id, p.repository, p.path, p.name, p.first_report_at, p.last_report_at,
                        (SELECT COUNT(*) FROM reports r WHERE r.project_id = p.id)
                 FROM projects p ORDER BY p.id",
            )
            .map_err(db_error)?;
        statement
            .query_map([], project_row)
            .map_err(db_error)?
            .collect::<std::result::Result<_, _>>()
            .map_err(db_error)
    }

    /// One project and its latest report as stored.
    pub fn project(&self, id: i64) -> Result<Option<(ProjectRow, serde_json::Value)>> {
        let conn = self.conn();
        let row = conn
            .query_row(
                "SELECT p.id, p.repository, p.path, p.name, p.first_report_at, p.last_report_at,
                        (SELECT COUNT(*) FROM reports r WHERE r.project_id = p.id)
                 FROM projects p WHERE p.id = ?1",
                params![id],
                project_row,
            )
            .optional()
            .map_err(db_error)?;
        let Some(row) = row else { return Ok(None) };
        let body: String = conn
            .query_row(
                "SELECT body FROM reports WHERE project_id = ?1 ORDER BY id DESC LIMIT 1",
                params![id],
                |row| row.get(0),
            )
            .map_err(db_error)?;
        Ok(Some((row, serde_json::from_str(&body)?)))
    }

    /// Create a publish key and return its secret. Only the SHA-256 of the
    /// secret is stored, so this is the one time the secret exists in full.
    pub fn create_key(&self, name: &str) -> Result<String> {
        if !valid_key_name(name) {
            return Err(
                TuffError::usage(format!("'{name}' is not a valid key name"))
                    .with_hint("use 1 to 64 letters, digits, '-', '_' or '.'"),
            );
        }
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|error| {
            TuffError::of(
                ErrorKind::Internal,
                format!("no random bytes for a key: {error}"),
            )
        })?;
        let secret = format!("{KEY_PREFIX}{}", hex(&bytes));
        let inserted = self.conn().execute(
            "INSERT INTO keys (name, sha256, created_at) VALUES (?1, ?2, ?3)",
            params![name, sha256_hex(secret.as_bytes()), now()],
        );
        match inserted {
            Ok(_) => Ok(secret),
            Err(rusqlite::Error::SqliteFailure(failure, _))
                if failure.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Err(
                    TuffError::refused(format!("a key named '{name}' already exists")).with_hint(
                        format!("run 'tuff console key revoke {name}' first, or pick another name"),
                    ),
                )
            }
            Err(error) => Err(db_error(error)),
        }
    }

    /// Keys in creation order.
    pub fn keys(&self) -> Result<Vec<KeyInfo>> {
        let conn = self.conn();
        let mut statement = conn
            .prepare("SELECT name, created_at, last_used_at FROM keys ORDER BY created_at, name")
            .map_err(db_error)?;
        statement
            .query_map([], |row| {
                Ok(KeyInfo {
                    name: row.get(0)?,
                    created_at: row.get(1)?,
                    last_used_at: row.get(2)?,
                })
            })
            .map_err(db_error)?
            .collect::<std::result::Result<_, _>>()
            .map_err(db_error)
    }

    pub fn key_count(&self) -> Result<u64> {
        self.conn()
            .query_row("SELECT COUNT(*) FROM keys", [], |row| row.get(0))
            .map_err(db_error)
    }

    pub fn revoke_key(&self, name: &str) -> Result<()> {
        let removed = self
            .conn()
            .execute("DELETE FROM keys WHERE name = ?1", params![name])
            .map_err(db_error)?;
        if removed == 0 {
            return Err(TuffError::not_found(format!("no key named '{name}'"))
                .with_hint("run 'tuff console key list' to see the names"));
        }
        Ok(())
    }

    /// Whether `secret` is a live key. A match records its use.
    pub fn verify_key(&self, secret: &str) -> Result<bool> {
        let updated = self
            .conn()
            .execute(
                "UPDATE keys SET last_used_at = ?2 WHERE sha256 = ?1",
                params![sha256_hex(secret.as_bytes()), now()],
            )
            .map_err(db_error)?;
        Ok(updated > 0)
    }
}

fn project_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectRow> {
    Ok(ProjectRow {
        id: row.get(0)?,
        repository: row.get(1)?,
        path: row.get(2)?,
        name: row.get(3)?,
        first_report_at: row.get(4)?,
        last_report_at: row.get(5)?,
        report_count: row.get(6)?,
    })
}

fn migrate(conn: &mut Connection, label: &str) -> Result<()> {
    let current: u32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(db_error)?;
    let known = MIGRATIONS.len() as u32;
    if current > known {
        return Err(TuffError::unsupported(format!(
            "{label} has schema version {current}, and this tuff reads up to {known}"
        ))
        .with_hint("upgrade tuff, or point --data at another folder"));
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction().map_err(db_error)?;
        tx.execute_batch(sql).map_err(db_error)?;
        tx.pragma_update(None, "user_version", index as u32 + 1)
            .map_err(db_error)?;
        tx.commit().map_err(db_error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tuff_core::report::ProjectIdentity;

    fn ingest(
        store: &Store,
        generated_at: &str,
        commit: &str,
        lockfile: serde_json::Value,
    ) -> IngestOutcome {
        let report = Report {
            schema: 1,
            tuff_version: "0.12.0".into(),
            generated_at: generated_at.into(),
            project: ProjectIdentity {
                repository: "github.com/acme/agents".into(),
                path: "apps/billing-agent".into(),
                name: "billing-agent".into(),
                commit: Some(commit.into()),
                branch: Some("main".into()),
                dirty: false,
            },
            lockfile,
            check: json!({ "valid": true, "results": [] }),
            outdated: None,
        };
        let raw = serde_json::to_value(&report).unwrap();
        store.ingest(&report, &raw).unwrap()
    }

    #[test]
    fn a_new_database_is_migrated_to_the_latest_schema() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len() as u32);
        assert!(store.projects().unwrap().is_empty());
    }

    #[test]
    fn a_database_from_a_newer_tuff_is_refused() {
        let temp = tempfile::tempdir().unwrap();
        {
            let store = Store::open(temp.path()).unwrap();
            store
                .conn()
                .pragma_update(None, "user_version", 99)
                .unwrap();
        }
        let error = Store::open(temp.path()).err().unwrap();
        assert_eq!(error.kind(), ErrorKind::Unsupported);
        assert!(error.hint().unwrap().contains("upgrade tuff"));
    }

    #[test]
    fn reopening_keeps_the_data() {
        let temp = tempfile::tempdir().unwrap();
        Store::open(temp.path()).unwrap().create_key("ci").unwrap();
        assert_eq!(Store::open(temp.path()).unwrap().key_count().unwrap(), 1);
    }

    #[test]
    fn the_first_report_creates_the_project() {
        let store = Store::open_in_memory().unwrap();
        let outcome = ingest(
            &store,
            "2026-09-16T18:00:00Z",
            "aaa",
            json!({ "version": 3 }),
        );
        assert!(outcome.project_first_seen);
        assert!(!outcome.deduplicated);
        let projects = store.projects().unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].repository, "github.com/acme/agents");
        assert_eq!(projects[0].path, "apps/billing-agent");
        assert_eq!(projects[0].report_count, 1);
    }

    #[test]
    fn a_report_equal_but_for_generated_at_is_deduplicated() {
        let store = Store::open_in_memory().unwrap();
        let first = ingest(
            &store,
            "2026-09-16T18:00:00Z",
            "aaa",
            json!({ "version": 3 }),
        );
        let second = ingest(
            &store,
            "2026-09-16T19:30:00Z",
            "aaa",
            json!({ "version": 3 }),
        );
        assert!(second.deduplicated);
        assert!(!second.project_first_seen);
        assert_eq!(second.report_id, first.report_id);
        assert_eq!(store.projects().unwrap()[0].report_count, 1);
    }

    #[test]
    fn a_changed_report_is_stored_and_a_return_to_an_older_one_too() {
        let store = Store::open_in_memory().unwrap();
        let a = ingest(
            &store,
            "2026-09-16T18:00:00Z",
            "aaa",
            json!({ "version": 3 }),
        );
        let b = ingest(
            &store,
            "2026-09-16T18:05:00Z",
            "bbb",
            json!({ "version": 3 }),
        );
        let c = ingest(
            &store,
            "2026-09-16T18:10:00Z",
            "aaa",
            json!({ "version": 3 }),
        );
        assert!(!b.deduplicated && !c.deduplicated);
        assert_ne!(a.report_id, b.report_id);
        assert_ne!(b.report_id, c.report_id);
        assert_eq!(store.projects().unwrap()[0].report_count, 3);
    }

    #[test]
    fn digests_ignore_key_order_and_generated_at_only() {
        let one = json!({ "generatedAt": "x", "b": [1, { "d": 1, "c": 2 }], "a": null });
        let two = json!({ "a": null, "generatedAt": "y", "b": [1, { "c": 2, "d": 1 }] });
        assert_eq!(report_digest(&one), report_digest(&two));
        let three = json!({ "a": null, "b": [1, { "c": 2, "d": 2 }] });
        assert_ne!(report_digest(&one), report_digest(&three));
    }

    #[test]
    fn the_latest_report_is_read_back_as_stored() {
        let store = Store::open_in_memory().unwrap();
        ingest(
            &store,
            "2026-09-16T18:00:00Z",
            "aaa",
            json!({ "version": 3 }),
        );
        let outcome = ingest(
            &store,
            "2026-09-16T18:05:00Z",
            "bbb",
            json!({ "version": 3 }),
        );
        let (project, latest) = store.project(outcome.project_id).unwrap().unwrap();
        assert_eq!(project.report_count, 2);
        assert_eq!(latest["project"]["commit"], "bbb");
        assert!(store.project(999).unwrap().is_none());
    }

    #[test]
    fn a_key_secret_is_shown_once_and_only_its_hash_is_stored() {
        let store = Store::open_in_memory().unwrap();
        let secret = store.create_key("ci").unwrap();
        assert!(secret.starts_with(KEY_PREFIX));
        assert_eq!(secret.len(), KEY_PREFIX.len() + 64);

        let stored: String = store
            .conn()
            .query_row("SELECT sha256 FROM keys WHERE name = 'ci'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(stored, sha256_hex(secret.as_bytes()));
        assert!(!stored.contains(&secret));

        assert!(store.verify_key(&secret).unwrap());
        assert!(!store.verify_key("tuffc_wrong").unwrap());
        assert!(!store.verify_key("").unwrap());
        let keys = store.keys().unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].name, "ci");
        assert!(keys[0].last_used_at.is_some());
    }

    #[test]
    fn a_key_is_unused_until_it_authenticates() {
        let store = Store::open_in_memory().unwrap();
        store.create_key("ci").unwrap();
        assert!(store.keys().unwrap()[0].last_used_at.is_none());
    }

    #[test]
    fn a_revoked_key_stops_working() {
        let store = Store::open_in_memory().unwrap();
        let secret = store.create_key("ci").unwrap();
        store.revoke_key("ci").unwrap();
        assert!(!store.verify_key(&secret).unwrap());
        assert_eq!(store.key_count().unwrap(), 0);
        let error = store.revoke_key("ci").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn key_names_are_unique_and_validated() {
        let store = Store::open_in_memory().unwrap();
        store.create_key("ci").unwrap();
        assert_eq!(
            store.create_key("ci").unwrap_err().kind(),
            ErrorKind::Refused
        );
        for bad in ["", "has space", "a/b", &"x".repeat(65)] {
            assert_eq!(
                store.create_key(bad).unwrap_err().kind(),
                ErrorKind::Usage,
                "{bad:?}"
            );
        }
        let one = store.create_key("one").unwrap();
        let two = store.create_key("two").unwrap();
        assert_ne!(one, two);
    }
}
