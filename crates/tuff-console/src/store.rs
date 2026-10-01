//! The console's SQLite file (RFC-108 D6).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tuff_core::error::{ErrorKind, Result, TuffError};
use tuff_core::report::{Report, normalize_remote};

use crate::events::{self, Snapshot};

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
    // 2: a key may be bound to one repository (D5).
    "ALTER TABLE keys ADD COLUMN repository TEXT;",
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
    /// The one repository the key may publish for, when it is scoped.
    pub repository: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

/// One stored report, without its body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportSummary {
    pub id: i64,
    pub received_at: String,
    pub generated_at: String,
    pub commit: Option<String>,
    pub branch: Option<String>,
    pub tuff_version: String,
    pub digest: String,
}

/// What a live key allows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyGrant {
    pub name: String,
    /// The repository the key is bound to, normalised; `None` for any.
    pub repository: Option<String>,
}

/// One change recorded between two consecutive reports of a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventRow {
    pub id: i64,
    pub project_id: i64,
    pub report_id: i64,
    pub kind: String,
    pub capability_type: Option<String>,
    pub capability_id: Option<String>,
    pub target: Option<String>,
    pub detail: Option<String>,
    pub commit: Option<String>,
    pub occurred_at: String,
}

/// Which events to list. Empty fields match everything.
#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    pub project_id: Option<i64>,
    pub capability: Option<String>,
    pub kind: Option<String>,
    /// RFC 3339 time or a date; events from then on.
    pub since: Option<String>,
    /// Only events older than this event id, for paging back.
    pub before: Option<i64>,
    pub limit: Option<u32>,
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

/// SHA-256 over the report's canonical JSON without `generatedAt` and
/// the project's `commit`, `branch`, and `dirty` (D6). Those describe when
/// and where the report was taken; two reports that differ only there say
/// the same thing about the project, so a CI job publishing on every push
/// does not add a row per commit.
pub fn report_digest(report: &serde_json::Value) -> String {
    let mut value = report.clone();
    if let Some(map) = value.as_object_mut() {
        map.remove("generatedAt");
        if let Some(project) = map.get_mut("project").and_then(|p| p.as_object_mut()) {
            for key in ["commit", "branch", "dirty"] {
                project.remove(key);
            }
        }
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
    /// (digest over everything except `generatedAt`, commit, branch, and
    /// dirty) adds no row: the previous row takes the new report's commit,
    /// branch, body, and times, so the project shows where it was last
    /// seen, and the project's last report time moves.
    pub fn ingest(&self, report: &Report, raw: &serde_json::Value) -> Result<IngestOutcome> {
        self.ingest_at(report, raw, &now())
    }

    /// [`Store::ingest`] with the time the report counts as received at,
    /// an RFC 3339 UTC string. `--demo` uses it to give sample reports a
    /// history.
    pub fn ingest_at(
        &self,
        report: &Report,
        raw: &serde_json::Value,
        received_at: &str,
    ) -> Result<IngestOutcome> {
        let digest = report_digest(raw);
        let body = serde_json::to_string(raw)?;
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

        let previous: Option<(i64, String, String)> = tx
            .query_row(
                "SELECT id, digest, body FROM reports WHERE project_id = ?1 ORDER BY id DESC LIMIT 1",
                params![project_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(db_error)?;

        let outcome = match previous {
            Some((report_id, previous_digest, _)) if previous_digest == digest => {
                tx.execute(
                    "UPDATE reports SET received_at = ?2, generated_at = ?3, commit_sha = ?4,
                                        branch = ?5, body = ?6
                     WHERE id = ?1",
                    params![
                        report_id,
                        received_at,
                        report.generated_at,
                        report.project.commit,
                        report.project.branch,
                        body
                    ],
                )
                .map_err(db_error)?;
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
            previous => {
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

                let current = Snapshot::from_report(raw);
                let before = previous
                    .and_then(|(_, _, body)| serde_json::from_str::<serde_json::Value>(&body).ok())
                    .map(|body| Snapshot::from_report(&body));
                for event in events::diff(before.as_ref(), &current) {
                    tx.execute(
                        "INSERT INTO events (project_id, report_id, kind, capability_type,
                                             capability_id, target, detail, commit_sha, occurred_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                        params![
                            project_id,
                            report_id,
                            event.kind,
                            event.capability_type,
                            event.capability_id,
                            event.target,
                            event.detail,
                            report.project.commit,
                            received_at
                        ],
                    )
                    .map_err(db_error)?;
                }
                tx.execute(
                    "DELETE FROM inventory WHERE project_id = ?1",
                    params![project_id],
                )
                .map_err(db_error)?;
                for row in &current.rows {
                    tx.execute(
                        "INSERT OR REPLACE INTO inventory (project_id, capability_type, capability_id,
                                                          target, version, source, status)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            project_id,
                            row.capability_type,
                            row.capability_id,
                            row.target,
                            row.version,
                            row.source,
                            row.status
                        ],
                    )
                    .map_err(db_error)?;
                }
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

    /// Every project with its latest report as stored, in the order the
    /// projects were first seen.
    pub fn latest_reports(&self) -> Result<Vec<(ProjectRow, serde_json::Value)>> {
        let conn = self.conn();
        let mut statement = conn
            .prepare(
                "SELECT p.id, p.repository, p.path, p.name, p.first_report_at, p.last_report_at,
                        (SELECT COUNT(*) FROM reports r WHERE r.project_id = p.id),
                        (SELECT body FROM reports r WHERE r.project_id = p.id
                         ORDER BY r.id DESC LIMIT 1)
                 FROM projects p ORDER BY p.id",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((project_row(row)?, row.get::<_, Option<String>>(7)?))
            })
            .map_err(db_error)?;
        let mut out = Vec::new();
        for row in rows {
            let (project, body) = row.map_err(db_error)?;
            if let Some(body) = body {
                out.push((project, serde_json::from_str(&body)?));
            }
        }
        Ok(out)
    }

    /// The stored reports of a project, newest first, without their bodies.
    pub fn report_history(&self, project_id: i64, limit: u32) -> Result<Vec<ReportSummary>> {
        let conn = self.conn();
        let mut statement = conn
            .prepare(
                "SELECT id, received_at, generated_at, commit_sha, branch, tuff_version, digest
                 FROM reports WHERE project_id = ?1 ORDER BY id DESC LIMIT ?2",
            )
            .map_err(db_error)?;
        statement
            .query_map(params![project_id, i64::from(limit)], |row| {
                Ok(ReportSummary {
                    id: row.get(0)?,
                    received_at: row.get(1)?,
                    generated_at: row.get(2)?,
                    commit: row.get(3)?,
                    branch: row.get(4)?,
                    tuff_version: row.get(5)?,
                    digest: row.get(6)?,
                })
            })
            .map_err(db_error)?
            .collect::<std::result::Result<_, _>>()
            .map_err(db_error)
    }

    /// Create a publish key and return its secret. Only the SHA-256 of the
    /// secret is stored, so this is the one time the secret exists in full.
    /// With `repository`, the key may publish for that repository only.
    pub fn create_key(&self, name: &str, repository: Option<&str>) -> Result<String> {
        if !valid_key_name(name) {
            return Err(
                TuffError::usage(format!("'{name}' is not a valid key name"))
                    .with_hint("use 1 to 64 letters, digits, '-', '_' or '.'"),
            );
        }
        let repository = match repository {
            Some(text) => {
                let normalized = normalize_remote(text);
                if !normalized.contains('/') {
                    return Err(
                        TuffError::usage(format!("'{text}' does not name a repository"))
                            .with_hint("use host/owner/name, such as github.com/acme/web"),
                    );
                }
                Some(normalized)
            }
            None => None,
        };
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|error| {
            TuffError::of(
                ErrorKind::Internal,
                format!("no random bytes for a key: {error}"),
            )
        })?;
        let secret = format!("{KEY_PREFIX}{}", hex(&bytes));
        let inserted = self.conn().execute(
            "INSERT INTO keys (name, sha256, created_at, repository) VALUES (?1, ?2, ?3, ?4)",
            params![name, sha256_hex(secret.as_bytes()), now(), repository],
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
            .prepare(
                "SELECT name, repository, created_at, last_used_at FROM keys
                 ORDER BY created_at, name",
            )
            .map_err(db_error)?;
        statement
            .query_map([], |row| {
                Ok(KeyInfo {
                    name: row.get(0)?,
                    repository: row.get(1)?,
                    created_at: row.get(2)?,
                    last_used_at: row.get(3)?,
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

    /// The grant of `secret` when it is a live key. A match records its use.
    pub fn verify_key(&self, secret: &str) -> Result<Option<KeyGrant>> {
        let hash = sha256_hex(secret.as_bytes());
        let conn = self.conn();
        let grant = conn
            .query_row(
                "SELECT name, repository FROM keys WHERE sha256 = ?1",
                params![hash],
                |row| {
                    Ok(KeyGrant {
                        name: row.get(0)?,
                        repository: row.get(1)?,
                    })
                },
            )
            .optional()
            .map_err(db_error)?;
        if grant.is_some() {
            conn.execute(
                "UPDATE keys SET last_used_at = ?2 WHERE sha256 = ?1",
                params![hash, now()],
            )
            .map_err(db_error)?;
        }
        Ok(grant)
    }

    /// Recorded events, newest first.
    pub fn events(&self, filter: &EventFilter) -> Result<Vec<EventRow>> {
        let conn = self.conn();
        let mut statement = conn
            .prepare(
                "SELECT e.id, e.project_id, e.report_id, e.kind, e.capability_type,
                        e.capability_id, e.target, e.detail, e.commit_sha, e.occurred_at
                 FROM events e
                 WHERE (?1 IS NULL OR e.project_id = ?1)
                   AND (?2 IS NULL OR e.capability_id = ?2)
                   AND (?3 IS NULL OR e.kind = ?3)
                   AND (?4 IS NULL OR e.occurred_at >= ?4)
                   AND (?6 IS NULL OR e.id < ?6)
                 ORDER BY e.id DESC
                 LIMIT ?5",
            )
            .map_err(db_error)?;
        statement
            .query_map(
                params![
                    filter.project_id,
                    filter.capability,
                    filter.kind,
                    filter.since,
                    i64::from(filter.limit.unwrap_or(500)),
                    filter.before
                ],
                |row| {
                    Ok(EventRow {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        report_id: row.get(2)?,
                        kind: row.get(3)?,
                        capability_type: row.get(4)?,
                        capability_id: row.get(5)?,
                        target: row.get(6)?,
                        detail: row.get(7)?,
                        commit: row.get(8)?,
                        occurred_at: row.get(9)?,
                    })
                },
            )
            .map_err(db_error)?
            .collect::<std::result::Result<_, _>>()
            .map_err(db_error)
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
        Store::open(temp.path())
            .unwrap()
            .create_key("ci", None)
            .unwrap();
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
            json!({ "version": 3, "capabilities": [] }),
        );
        let c = ingest(
            &store,
            "2026-09-16T18:10:00Z",
            "ccc",
            json!({ "version": 3 }),
        );
        assert!(!b.deduplicated && !c.deduplicated);
        assert_ne!(a.report_id, b.report_id);
        assert_ne!(b.report_id, c.report_id);
        assert_eq!(store.projects().unwrap()[0].report_count, 3);
    }

    #[test]
    fn a_new_commit_with_the_same_content_moves_the_latest_report() {
        let store = Store::open_in_memory().unwrap();
        let first = ingest(
            &store,
            "2026-09-16T18:00:00Z",
            "aaa",
            json!({ "version": 3 }),
        );
        let second = ingest(
            &store,
            "2026-09-17T09:00:00Z",
            "bbb",
            json!({ "version": 3 }),
        );
        assert!(second.deduplicated);
        assert_eq!(second.report_id, first.report_id);
        let reports = store.report_history(first.project_id, 10).unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].commit.as_deref(), Some("bbb"));
        assert_eq!(reports[0].generated_at, "2026-09-17T09:00:00Z");
    }

    #[test]
    fn digests_ignore_key_order_and_generated_at_only() {
        let one = json!({ "generatedAt": "x", "b": [1, { "d": 1, "c": 2 }], "a": null });
        let two = json!({ "a": null, "generatedAt": "y", "b": [1, { "c": 2, "d": 1 }] });
        assert_eq!(report_digest(&one), report_digest(&two));
        let three = json!({ "a": null, "b": [1, { "c": 2, "d": 2 }] });
        assert_ne!(report_digest(&one), report_digest(&three));
        let at = |commit: &str, dirty: bool| json!({ "project": { "repository": "r", "commit": commit, "branch": "main", "dirty": dirty }, "x": 1 });
        assert_eq!(
            report_digest(&at("aaa", false)),
            report_digest(&at("bbb", true))
        );
        let elsewhere = json!({ "project": { "repository": "s", "commit": "aaa" }, "x": 1 });
        assert_ne!(report_digest(&at("aaa", false)), report_digest(&elsewhere));
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
        assert_eq!(project.report_count, 1, "only the commit changed");
        assert_eq!(latest["project"]["commit"], "bbb");
        assert!(store.project(999).unwrap().is_none());
    }

    #[test]
    fn a_key_secret_is_shown_once_and_only_its_hash_is_stored() {
        let store = Store::open_in_memory().unwrap();
        let secret = store.create_key("ci", None).unwrap();
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

        assert!(store.verify_key(&secret).unwrap().is_some());
        assert!(store.verify_key("tuffc_wrong").unwrap().is_none());
        assert!(store.verify_key("").unwrap().is_none());
        let keys = store.keys().unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].name, "ci");
        assert!(keys[0].last_used_at.is_some());
    }

    #[test]
    fn a_key_is_unused_until_it_authenticates() {
        let store = Store::open_in_memory().unwrap();
        store.create_key("ci", None).unwrap();
        assert!(store.keys().unwrap()[0].last_used_at.is_none());
    }

    #[test]
    fn a_revoked_key_stops_working() {
        let store = Store::open_in_memory().unwrap();
        let secret = store.create_key("ci", None).unwrap();
        store.revoke_key("ci").unwrap();
        assert!(store.verify_key(&secret).unwrap().is_none());
        assert_eq!(store.key_count().unwrap(), 0);
        let error = store.revoke_key("ci").unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn key_names_are_unique_and_validated() {
        let store = Store::open_in_memory().unwrap();
        store.create_key("ci", None).unwrap();
        assert_eq!(
            store.create_key("ci", None).unwrap_err().kind(),
            ErrorKind::Refused
        );
        for bad in ["", "has space", "a/b", &"x".repeat(65)] {
            assert_eq!(
                store.create_key(bad, None).unwrap_err().kind(),
                ErrorKind::Usage,
                "{bad:?}"
            );
        }
        let one = store.create_key("one", None).unwrap();
        let two = store.create_key("two", None).unwrap();
        assert_ne!(one, two);
    }

    fn carrying(
        commit: &str,
        version: &str,
        status: &str,
        gap: bool,
    ) -> (Report, serde_json::Value) {
        let mut policy = json!({
            "name": "guard", "type": "policy", "target": "codex", "version": "1",
            "source": { "kind": "local", "path": "guard" },
        });
        if gap {
            policy["unenforced_rules"] =
                json!([{ "rule": 2, "description": "deny read", "reason": "none" }]);
        }
        let report = Report {
            schema: 1,
            tuff_version: "0.12.0".into(),
            generated_at: "2026-09-30T10:00:00Z".into(),
            project: ProjectIdentity {
                repository: "github.com/acme/agents".into(),
                path: ".".into(),
                name: "agents".into(),
                commit: Some(commit.into()),
                branch: None,
                dirty: false,
            },
            lockfile: json!({ "version": 3, "capabilities": [
                { "name": "lint", "type": "skill", "target": "claude", "version": version,
                  "source": { "kind": "local", "path": "lint" } },
                policy,
            ] }),
            check: json!({ "valid": true, "results": [
                { "id": "lint", "type": "skill", "target": "claude", "status": status },
            ] }),
            outdated: None,
        };
        let raw = serde_json::to_value(&report).unwrap();
        (report, raw)
    }

    fn kinds_of(store: &Store) -> Vec<String> {
        let mut rows = store.events(&EventFilter::default()).unwrap();
        rows.reverse();
        rows.into_iter().map(|row| row.kind).collect()
    }

    #[test]
    fn two_different_reports_record_the_events_between_them() {
        let store = Store::open_in_memory().unwrap();
        let (report, raw) = carrying("aaa", "1.0.0", "ok", true);
        store.ingest(&report, &raw).unwrap();
        assert_eq!(
            kinds_of(&store),
            [
                "project_first_seen",
                "capability_added",
                "capability_added",
                "policy_gap_added"
            ]
        );

        let (report, raw) = carrying("bbb", "1.1.0", "modified", false);
        let outcome = store.ingest(&report, &raw).unwrap();
        let all = store.events(&EventFilter::default()).unwrap();
        let second: Vec<_> = all
            .iter()
            .filter(|row| row.report_id == outcome.report_id)
            .collect();
        let mut kinds: Vec<&str> = second.iter().map(|row| row.kind.as_str()).collect();
        kinds.sort_unstable();
        assert_eq!(
            kinds,
            ["drift_detected", "policy_gap_closed", "version_changed"]
        );
        assert!(
            second
                .iter()
                .all(|row| row.commit.as_deref() == Some("bbb"))
        );
        assert!(
            second
                .iter()
                .all(|row| row.project_id == outcome.project_id)
        );

        let (report, raw) = carrying("ccc", "1.1.0", "ok", false);
        store.ingest(&report, &raw).unwrap();
        assert_eq!(
            kinds_of(&store).last().map(String::as_str),
            Some("drift_cleared")
        );
    }

    #[test]
    fn the_same_report_twice_records_nothing_the_second_time() {
        let store = Store::open_in_memory().unwrap();
        let (report, raw) = carrying("aaa", "1.0.0", "ok", true);
        store.ingest(&report, &raw).unwrap();
        let before = store.events(&EventFilter::default()).unwrap();
        let mut again = raw.clone();
        again["generatedAt"] = json!("2026-10-01T00:00:00Z");
        let outcome = store.ingest(&report, &again).unwrap();
        assert!(outcome.deduplicated);
        assert_eq!(store.events(&EventFilter::default()).unwrap(), before);
    }

    #[test]
    fn the_inventory_holds_the_latest_report_as_rows() {
        let store = Store::open_in_memory().unwrap();
        let (report, raw) = carrying("aaa", "1.0.0", "ok", false);
        store.ingest(&report, &raw).unwrap();
        let (report, raw) = carrying("bbb", "1.1.0", "modified", false);
        store.ingest(&report, &raw).unwrap();
        type Row = (String, String, String, String, String);
        let rows: Vec<Row> = store
            .conn()
            .prepare(
                "SELECT capability_type, capability_id, target, version, status
                 FROM inventory ORDER BY capability_id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[1],
            (
                "skill".into(),
                "lint".into(),
                "claude".into(),
                "1.1.0".into(),
                "modified".into()
            )
        );
    }

    #[test]
    fn events_filter_by_project_capability_kind_and_time() {
        let store = Store::open_in_memory().unwrap();
        let (report, raw) = carrying("aaa", "1.0.0", "ok", true);
        let outcome = store.ingest(&report, &raw).unwrap();
        let count = |filter: EventFilter| store.events(&filter).unwrap().len();
        assert_eq!(
            count(EventFilter {
                project_id: Some(outcome.project_id),
                ..Default::default()
            }),
            4
        );
        assert_eq!(
            count(EventFilter {
                project_id: Some(999),
                ..Default::default()
            }),
            0
        );
        assert_eq!(
            count(EventFilter {
                capability: Some("lint".into()),
                ..Default::default()
            }),
            1
        );
        assert_eq!(
            count(EventFilter {
                kind: Some("capability_added".into()),
                ..Default::default()
            }),
            2
        );
        assert_eq!(
            count(EventFilter {
                since: Some("2999-01-01".into()),
                ..Default::default()
            }),
            0
        );
        assert_eq!(
            count(EventFilter {
                since: Some("2000-01-01".into()),
                limit: Some(1),
                ..Default::default()
            }),
            1
        );
    }

    #[test]
    fn a_scoped_key_remembers_its_repository() {
        let store = Store::open_in_memory().unwrap();
        let secret = store
            .create_key("web", Some("git@github.com:Acme/web.git"))
            .unwrap();
        let grant = store.verify_key(&secret).unwrap().unwrap();
        assert_eq!(grant.repository.as_deref(), Some("github.com/Acme/web"));
        assert_eq!(
            store.keys().unwrap()[0].repository.as_deref(),
            Some("github.com/Acme/web")
        );
        assert_eq!(
            store.create_key("bad", Some("web")).unwrap_err().kind(),
            ErrorKind::Usage
        );
    }

    #[test]
    fn a_version_one_database_gains_the_key_scope_column() {
        let temp = tempfile::tempdir().unwrap();
        {
            let conn = Connection::open(temp.path().join(DATABASE_FILE)).unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
            conn.execute(
                "INSERT INTO keys (name, sha256, created_at) VALUES ('old', 'x', 't')",
                [],
            )
            .unwrap();
        }
        let store = Store::open(temp.path()).unwrap();
        assert_eq!(store.schema_version().unwrap(), 2);
        let keys = store.keys().unwrap();
        assert_eq!(keys[0].name, "old");
        assert_eq!(keys[0].repository, None);
    }
}
