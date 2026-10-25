//! Lokalny store SQLite. Faza 0: historia zapytań i dokumenty.
//!
//! `Store` jest `Send + Sync` dzięki `Mutex<Connection>`; pojedyncze zapytania
//! są krótkie, więc synchronizacja jest tańsza niż drugi runtime.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("store lock poisoned")]
    Lock,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunHit {
    pub canonical_url: String,
    pub title: String,
    pub domain: String,
    pub rank: usize,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: String,
    pub query_text: String,
    pub query_hash: String,
    pub providers: Vec<String>,
    pub hit_count: usize,
    pub latency_ms: u64,
    pub refused: bool,
    pub created_at: String,
    pub hits: Vec<RunHit>,
}

impl RunRecord {
    pub fn new(query_text: impl Into<String>, query_hash: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            query_text: query_text.into(),
            query_hash: query_hash.into(),
            providers: Vec::new(),
            hit_count: 0,
            latency_ms: 0,
            refused: false,
            created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            hits: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub id: String,
    pub query_text: String,
    pub providers: Vec<String>,
    pub hit_count: usize,
    pub latency_ms: u64,
    pub refused: bool,
    pub created_at: String,
}

/// Granica backendu historii. Faza 0 ma tylko SQLite; trait pozwala później
/// podmienić store (np. Postgres w trybie server) bez zmian w pipeline.
pub trait RunStore: Send + Sync {
    fn record_run(&self, run: &RunRecord) -> Result<(), StoreError>;
    fn recent_runs(&self, limit: usize) -> Result<Vec<RunSummary>, StoreError>;
    fn document_count(&self) -> Result<usize, StoreError>;
}

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        if path != Path::new(":memory:") {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
        }
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;",
        )?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        restrict_permissions(path);
        Ok(store)
    }

    fn migrate(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS runs (
                id           TEXT PRIMARY KEY,
                query_text   TEXT NOT NULL,
                query_hash   TEXT NOT NULL,
                providers    TEXT NOT NULL,
                hit_count    INTEGER NOT NULL,
                latency_ms   INTEGER NOT NULL,
                refused      INTEGER NOT NULL,
                created_at   TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS documents (
                id            TEXT PRIMARY KEY,
                canonical_url TEXT NOT NULL UNIQUE,
                title         TEXT NOT NULL,
                domain        TEXT NOT NULL,
                first_seen    TEXT NOT NULL,
                last_seen     TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS run_documents (
                run_id      TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
                document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
                rank        INTEGER NOT NULL,
                score       REAL NOT NULL,
                PRIMARY KEY (run_id, document_id)
            );

            CREATE INDEX IF NOT EXISTS idx_runs_created ON runs(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_documents_domain ON documents(domain);
            "#,
        )?;
        Ok(())
    }

    pub fn record_run(&self, run: &RunRecord) -> Result<(), StoreError> {
        let mut conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let tx = conn.transaction()?;

        let providers = serde_json::to_string(&run.providers).unwrap_or_else(|_| "[]".to_string());
        tx.execute(
            "INSERT INTO runs (id, query_text, query_hash, providers, hit_count, latency_ms, refused, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                run.id,
                run.query_text,
                run.query_hash,
                providers,
                run.hit_count as i64,
                run.latency_ms as i64,
                if run.refused { 1i64 } else { 0i64 },
                run.created_at,
            ],
        )?;

        for hit in &run.hits {
            tx.execute(
                "INSERT INTO documents (id, canonical_url, title, domain, first_seen, last_seen)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT(canonical_url) DO UPDATE SET
                    title = excluded.title,
                    last_seen = excluded.last_seen",
                params![
                    Uuid::new_v4().to_string(),
                    hit.canonical_url,
                    hit.title,
                    hit.domain,
                    run.created_at,
                ],
            )?;

            let document_id: String = tx.query_row(
                "SELECT id FROM documents WHERE canonical_url = ?1",
                params![hit.canonical_url],
                |row| row.get(0),
            )?;

            tx.execute(
                "INSERT OR REPLACE INTO run_documents (run_id, document_id, rank, score)
                 VALUES (?1, ?2, ?3, ?4)",
                params![run.id, document_id, hit.rank as i64, hit.score as f64],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn recent_runs(&self, limit: usize) -> Result<Vec<RunSummary>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let mut stmt = conn.prepare(
            "SELECT id, query_text, providers, hit_count, latency_ms, refused, created_at
             FROM runs ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            let providers: String = row.get(2)?;
            Ok(RunSummary {
                id: row.get(0)?,
                query_text: row.get(1)?,
                providers: serde_json::from_str(&providers).unwrap_or_default(),
                hit_count: row.get::<_, i64>(3)? as usize,
                latency_ms: row.get::<_, i64>(4)? as u64,
                refused: row.get::<_, i64>(5)? != 0,
                created_at: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn document_count(&self) -> Result<usize, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let count: Option<i64> = conn
            .query_row("SELECT COUNT(*) FROM documents", [], |row| row.get(0))
            .optional()?;
        Ok(count.unwrap_or(0) as usize)
    }
}

/// SQLite tworzy pliki z domyślnym umask; historia zapytań jest lokalna i prywatna,
/// więc ustawiamy 0600 także dla WAL/SHM. Błąd uprawnień nie blokuje startu.
fn restrict_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        if path == Path::new(":memory:") {
            return;
        }
        let mode = std::fs::Permissions::from_mode(0o600);
        let _ = std::fs::set_permissions(path, mode.clone());
        for suffix in ["-wal", "-shm"] {
            let sidecar = PathBuf::from(format!("{}{suffix}", path.display()));
            if sidecar.exists() {
                let _ = std::fs::set_permissions(sidecar, mode.clone());
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

impl RunStore for Store {
    fn record_run(&self, run: &RunRecord) -> Result<(), StoreError> {
        Store::record_run(self, run)
    }

    fn recent_runs(&self, limit: usize) -> Result<Vec<RunSummary>, StoreError> {
        Store::recent_runs(self, limit)
    }

    fn document_count(&self) -> Result<usize, StoreError> {
        Store::document_count(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(url: &str, rank: usize) -> RunHit {
        RunHit {
            canonical_url: url.to_string(),
            title: format!("doc {rank}"),
            domain: "example.org".to_string(),
            rank,
            score: 1.0 / rank as f32,
        }
    }

    #[test]
    fn records_and_reads_run() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("bald.db")).unwrap();
        let mut run = RunRecord::new("teoria", "sha256:abc");
        run.hits = vec![
            hit("https://example.org/a", 1),
            hit("https://example.org/b", 2),
        ];
        run.hit_count = 2;
        run.providers = vec!["demo".to_string()];
        store.record_run(&run).unwrap();

        let runs = store.recent_runs(10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].hit_count, 2);
        assert_eq!(store.document_count().unwrap(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn db_file_is_private() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bald.db");
        let _store = Store::open(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn same_url_is_one_document() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("bald.db")).unwrap();
        for i in 0..2 {
            let mut run = RunRecord::new(format!("q{i}"), format!("h{i}"));
            run.hits = vec![hit("https://example.org/a", 1)];
            run.hit_count = 1;
            store.record_run(&run).unwrap();
        }
        assert_eq!(store.document_count().unwrap(), 1);
    }
}
