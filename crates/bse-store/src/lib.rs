//! Lokalny store SQLite. Faza 0: historia zapytań i dokumenty.
//!
//! `Store` jest `Send + Sync` dzięki `Mutex<Connection>`; pojedyncze zapytania
//! są krótkie, więc synchronizacja jest tańsza niż drugi runtime.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use bse_core::{ClaimKind, EntityKind, EvidenceBundle, NewClaim, Stance};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("store lock poisoned")]
    Lock,

    #[error("ambiguous claim id prefix: {0}")]
    AmbiguousClaim(String),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceWrite {
    pub document_id: String,
    pub claims_upserted: usize,
    pub evidence_upserted: usize,
    pub entities_upserted: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StanceCount {
    pub stance: Stance,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimSummary {
    pub id: String,
    pub text: String,
    pub kind: ClaimKind,
    pub lang: Option<String>,
    pub status: String,
    pub document_count: usize,
    pub evidence_count: usize,
    pub first_seen: String,
    pub last_seen: String,
    pub stances: Vec<StanceCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceItem {
    pub document_id: String,
    pub canonical_url: String,
    pub title: String,
    pub domain: String,
    pub stance: Stance,
    pub strength: f32,
    pub quote: String,
    pub quote_hash: String,
    pub span_start: Option<usize>,
    pub span_end: Option<usize>,
    pub model_version: String,
    pub prompt_version: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityItem {
    pub id: String,
    pub kind: EntityKind,
    pub name: String,
    pub normalized: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimDetail {
    pub claim: ClaimSummary,
    pub evidence: Vec<EvidenceItem>,
    pub entities: Vec<EntityItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerStats {
    pub claims: usize,
    pub evidence: usize,
    pub entities: usize,
    pub documents_with_evidence: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchBot {
    pub id: String,
    pub name: String,
    pub query: String,
    pub interval_mins: u32,
    pub enabled: bool,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub run_count: u64,
    pub claims_found: u64,
    pub created_at: String,
    pub max_sources_per_run: usize,
    pub auto_ingest: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewResearchBot {
    pub name: String,
    pub query: String,
    pub interval_mins: u32,
    pub max_sources_per_run: Option<usize>,
    pub auto_ingest: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotRunRecord {
    pub id: String,
    pub bot_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub urls_checked: usize,
    pub claims_added: usize,
    pub error_msg: Option<String>,
    pub status: String,
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

            CREATE TABLE IF NOT EXISTS claims (
                id             TEXT PRIMARY KEY,
                text           TEXT NOT NULL,
                normalized     TEXT NOT NULL,
                lang           TEXT,
                kind           TEXT NOT NULL,
                status         TEXT NOT NULL DEFAULT 'unverified',
                first_seen     TEXT NOT NULL,
                last_seen      TEXT NOT NULL,
                document_count INTEGER NOT NULL DEFAULT 0,
                evidence_count INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS evidence (
                claim_id       TEXT NOT NULL REFERENCES claims(id) ON DELETE CASCADE,
                document_id    TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
                stance         TEXT NOT NULL,
                strength       REAL NOT NULL,
                quote          TEXT NOT NULL,
                quote_hash     TEXT NOT NULL,
                span_start     INTEGER,
                span_end       INTEGER,
                model_version  TEXT NOT NULL,
                prompt_version TEXT NOT NULL,
                created_at     TEXT NOT NULL,
                PRIMARY KEY (claim_id, document_id, quote_hash)
            );

            CREATE TABLE IF NOT EXISTS entities (
                id              TEXT PRIMARY KEY,
                kind            TEXT NOT NULL,
                name            TEXT NOT NULL,
                normalized_name TEXT NOT NULL,
                first_seen      TEXT NOT NULL,
                last_seen       TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS claim_entities (
                claim_id  TEXT NOT NULL REFERENCES claims(id) ON DELETE CASCADE,
                entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
                PRIMARY KEY (claim_id, entity_id)
            );

            CREATE INDEX IF NOT EXISTS idx_runs_created ON runs(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_documents_domain ON documents(domain);
            CREATE INDEX IF NOT EXISTS idx_claims_last_seen ON claims(last_seen DESC);
            CREATE INDEX IF NOT EXISTS idx_claims_documents ON claims(document_count DESC);
            CREATE INDEX IF NOT EXISTS idx_evidence_document ON evidence(document_id);
            CREATE INDEX IF NOT EXISTS idx_claim_entities_entity ON claim_entities(entity_id);

            CREATE TABLE IF NOT EXISTS research_bots (
                id                  TEXT PRIMARY KEY,
                name                TEXT NOT NULL,
                query               TEXT NOT NULL,
                interval_mins       INTEGER NOT NULL DEFAULT 360,
                enabled             INTEGER NOT NULL DEFAULT 1,
                last_run_at         TEXT,
                next_run_at         TEXT,
                run_count           INTEGER NOT NULL DEFAULT 0,
                claims_found        INTEGER NOT NULL DEFAULT 0,
                created_at          TEXT NOT NULL,
                max_sources_per_run INTEGER NOT NULL DEFAULT 3,
                auto_ingest         INTEGER NOT NULL DEFAULT 1
            );

            CREATE TABLE IF NOT EXISTS bot_runs (
                id           TEXT PRIMARY KEY,
                bot_id       TEXT NOT NULL REFERENCES research_bots(id) ON DELETE CASCADE,
                started_at   TEXT NOT NULL,
                finished_at  TEXT NOT NULL,
                urls_checked INTEGER NOT NULL DEFAULT 0,
                claims_added INTEGER NOT NULL DEFAULT 0,
                error_msg    TEXT,
                status       TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_bot_runs_bot ON bot_runs(bot_id, started_at DESC);
            CREATE INDEX IF NOT EXISTS idx_research_bots_due ON research_bots(enabled, next_run_at);
            "#,
        )?;

        // Faza 0 created `documents` without provenance columns. Add them in place
        // so existing databases keep their history.
        ensure_column(
            &conn,
            "documents",
            "content_hash",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(&conn, "documents", "lang", "TEXT")?;
        ensure_column(&conn, "documents", "published", "TEXT")?;
        ensure_column(&conn, "documents", "fetched_at", "TEXT")?;
        ensure_column(&conn, "documents", "excerpt", "TEXT")?;
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

    /// Zapisuje dokument i tylko te twierdzenia, które przeszły `CitationGate`.
    /// Operacja jest transakcyjna; powtórny zapis tego samego cytatu nie dubluje
    /// evidence, ale odświeża jego metadane.
    pub fn record_evidence(&self, bundle: &EvidenceBundle) -> Result<EvidenceWrite, StoreError> {
        let mut conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let tx = conn.transaction()?;
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);

        tx.execute(
            "INSERT INTO documents
                (id, canonical_url, title, domain, first_seen, last_seen,
                 content_hash, lang, published, fetched_at, excerpt)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(canonical_url) DO UPDATE SET
                title = excluded.title,
                domain = excluded.domain,
                last_seen = excluded.last_seen,
                content_hash = excluded.content_hash,
                lang = COALESCE(excluded.lang, documents.lang),
                published = COALESCE(excluded.published, documents.published),
                fetched_at = excluded.fetched_at,
                excerpt = excluded.excerpt",
            params![
                Uuid::new_v4().to_string(),
                bundle.document.canonical_url,
                bundle.document.title,
                bundle.document.domain,
                now,
                bundle.document.content_hash,
                bundle.document.lang,
                bundle.document.published,
                bundle.document.fetched_at,
                bundle.document.excerpt,
            ],
        )?;

        let document_id: String = tx.query_row(
            "SELECT id FROM documents WHERE canonical_url = ?1",
            params![bundle.document.canonical_url],
            |row| row.get(0),
        )?;

        let mut claims_upserted = 0usize;
        let mut entity_ids: HashSet<&str> = HashSet::new();
        for claim in &bundle.claims {
            upsert_claim(&tx, claim, &now)?;
            claims_upserted += 1;
            for entity in &claim.entities {
                tx.execute(
                    "INSERT INTO entities (id, kind, name, normalized_name, first_seen, last_seen)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                     ON CONFLICT(id) DO UPDATE SET
                        kind = excluded.kind,
                        name = excluded.name,
                        normalized_name = excluded.normalized_name,
                        last_seen = excluded.last_seen",
                    params![
                        entity.id,
                        entity.kind.as_str(),
                        entity.name,
                        entity.normalized,
                        now,
                    ],
                )?;
                tx.execute(
                    "INSERT OR IGNORE INTO claim_entities (claim_id, entity_id)
                     VALUES (?1, ?2)",
                    params![claim.id, entity.id],
                )?;
                entity_ids.insert(entity.id.as_str());
            }
        }

        let mut evidence_upserted = 0usize;
        for item in &bundle.evidence {
            tx.execute(
                "INSERT INTO evidence
                    (claim_id, document_id, stance, strength, quote, quote_hash,
                     span_start, span_end, model_version, prompt_version, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(claim_id, document_id, quote_hash) DO UPDATE SET
                    stance = excluded.stance,
                    strength = excluded.strength,
                    quote = excluded.quote,
                    span_start = excluded.span_start,
                    span_end = excluded.span_end,
                    model_version = excluded.model_version,
                    prompt_version = excluded.prompt_version,
                    created_at = excluded.created_at",
                params![
                    item.claim_id,
                    document_id,
                    item.stance.as_str(),
                    f64::from(item.strength),
                    item.quote,
                    item.quote_hash,
                    item.span_start.map(i64::from),
                    item.span_end.map(i64::from),
                    bundle.model_version,
                    bundle.prompt_version,
                    now,
                ],
            )?;
            evidence_upserted += 1;
        }

        let mut touched: HashSet<&str> = HashSet::new();
        for claim in &bundle.claims {
            touched.insert(claim.id.as_str());
        }
        for item in &bundle.evidence {
            touched.insert(item.claim_id.as_str());
        }
        for claim_id in touched {
            tx.execute(
                "UPDATE claims SET
                    document_count = (SELECT COUNT(DISTINCT document_id) FROM evidence WHERE claim_id = ?1),
                    evidence_count = (SELECT COUNT(*) FROM evidence WHERE claim_id = ?1)
                 WHERE id = ?1",
                params![claim_id],
            )?;
        }

        tx.commit()?;
        Ok(EvidenceWrite {
            document_id,
            claims_upserted,
            evidence_upserted,
            entities_upserted: entity_ids.len(),
        })
    }

    pub fn list_claims(&self, limit: usize) -> Result<Vec<ClaimSummary>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let ids = {
            let mut stmt = conn.prepare(
                "SELECT id FROM claims
                 ORDER BY document_count DESC, evidence_count DESC, last_seen DESC
                 LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit as i64], |row| row.get::<_, String>(0))?;
            let mut ids = Vec::new();
            for row in rows {
                ids.push(row?);
            }
            ids
        };

        let mut out = Vec::new();
        for id in ids {
            if let Some(summary) = load_claim_summary(&conn, &id)? {
                out.push(summary);
            }
        }
        Ok(out)
    }

    pub fn claim_detail(&self, claim_id: &str) -> Result<Option<ClaimDetail>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let Some(claim) = load_claim_summary(&conn, claim_id)? else {
            return Ok(None);
        };

        let evidence = {
            let mut stmt = conn.prepare(
                "SELECT e.document_id, d.canonical_url, d.title, d.domain,
                        e.stance, e.strength, e.quote, e.quote_hash,
                        e.span_start, e.span_end, e.model_version, e.prompt_version, e.created_at
                 FROM evidence e
                 JOIN documents d ON d.id = e.document_id
                 WHERE e.claim_id = ?1
                 ORDER BY e.created_at DESC, d.domain ASC",
            )?;
            let rows = stmt.query_map(params![claim_id], |row| {
                let stance: String = row.get(4)?;
                Ok(EvidenceItem {
                    document_id: row.get(0)?,
                    canonical_url: row.get(1)?,
                    title: row.get(2)?,
                    domain: row.get(3)?,
                    stance: Stance::parse(&stance).unwrap_or(Stance::Mentions),
                    strength: row.get::<_, f64>(5)? as f32,
                    quote: row.get(6)?,
                    quote_hash: row.get(7)?,
                    span_start: row.get::<_, Option<i64>>(8)?.map(|value| value as usize),
                    span_end: row.get::<_, Option<i64>>(9)?.map(|value| value as usize),
                    model_version: row.get(10)?,
                    prompt_version: row.get(11)?,
                    created_at: row.get(12)?,
                })
            })?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            out
        };

        let entities = {
            let mut stmt = conn.prepare(
                "SELECT en.id, en.kind, en.name, en.normalized_name
                 FROM entities en
                 JOIN claim_entities ce ON ce.entity_id = en.id
                 WHERE ce.claim_id = ?1
                 ORDER BY en.kind ASC, en.name ASC",
            )?;
            let rows = stmt.query_map(params![claim_id], |row| {
                let kind: String = row.get(1)?;
                Ok(EntityItem {
                    id: row.get(0)?,
                    kind: EntityKind::parse(&kind).unwrap_or(EntityKind::Other),
                    name: row.get(2)?,
                    normalized: row.get(3)?,
                })
            })?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            out
        };

        Ok(Some(ClaimDetail {
            claim,
            evidence,
            entities,
        }))
    }

    pub fn resolve_claim_id(&self, prefix: &str) -> Result<Option<String>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let mut stmt = conn.prepare(
            "SELECT id FROM claims
             WHERE id = ?1 OR id LIKE ?2 OR id LIKE ?3
             ORDER BY CASE WHEN id = ?1 THEN 0 ELSE 1 END
             LIMIT 2",
        )?;
        let pattern = format!("{prefix}%");
        let prefixed_pattern = format!("sha256:{prefix}%");
        let rows = stmt.query_map(params![prefix, pattern, prefixed_pattern], |row| {
            row.get::<_, String>(0)
        })?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row?);
        }
        match ids.len() {
            0 => Ok(None),
            1 => Ok(ids.pop()),
            _ => Err(StoreError::AmbiguousClaim(prefix.to_string())),
        }
    }

    pub fn ledger_stats(&self) -> Result<LedgerStats, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let claims = count_query(&conn, "SELECT COUNT(*) FROM claims")?;
        let evidence = count_query(&conn, "SELECT COUNT(*) FROM evidence")?;
        let entities = count_query(&conn, "SELECT COUNT(*) FROM entities")?;
        let documents_with_evidence =
            count_query(&conn, "SELECT COUNT(DISTINCT document_id) FROM evidence")?;
        Ok(LedgerStats {
            claims,
            evidence,
            entities,
            documents_with_evidence,
        })
    }

    pub fn create_bot(&self, new_bot: &NewResearchBot) -> Result<ResearchBot, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let max_sources = new_bot.max_sources_per_run.unwrap_or(3).clamp(1, 20);
        let auto_ingest = new_bot.auto_ingest.unwrap_or(true);
        let interval_mins = new_bot.interval_mins.max(1);

        conn.execute(
            "INSERT INTO research_bots (id, name, query, interval_mins, enabled, last_run_at, next_run_at, run_count, claims_found, created_at, max_sources_per_run, auto_ingest)
             VALUES (?1, ?2, ?3, ?4, 1, NULL, ?5, 0, 0, ?6, ?7, ?8)",
            params![
                id,
                new_bot.name.trim(),
                new_bot.query.trim(),
                interval_mins as i64,
                now,
                now,
                max_sources as i64,
                if auto_ingest { 1 } else { 0 },
            ],
        )?;

        Ok(ResearchBot {
            id,
            name: new_bot.name.trim().to_string(),
            query: new_bot.query.trim().to_string(),
            interval_mins,
            enabled: true,
            last_run_at: None,
            next_run_at: Some(now.clone()),
            run_count: 0,
            claims_found: 0,
            created_at: now,
            max_sources_per_run: max_sources,
            auto_ingest,
        })
    }

    pub fn list_bots(&self) -> Result<Vec<ResearchBot>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let mut stmt = conn.prepare(
            "SELECT id, name, query, interval_mins, enabled, last_run_at, next_run_at, run_count, claims_found, created_at, max_sources_per_run, auto_ingest
             FROM research_bots ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let enabled_int: i64 = row.get(4)?;
            let auto_ingest_int: i64 = row.get(11)?;
            let interval_mins_int: i64 = row.get(3)?;
            let run_count_int: i64 = row.get(7)?;
            let claims_found_int: i64 = row.get(8)?;
            let max_sources_int: i64 = row.get(10)?;
            Ok(ResearchBot {
                id: row.get(0)?,
                name: row.get(1)?,
                query: row.get(2)?,
                interval_mins: interval_mins_int as u32,
                enabled: enabled_int == 1,
                last_run_at: row.get(5)?,
                next_run_at: row.get(6)?,
                run_count: run_count_int as u64,
                claims_found: claims_found_int as u64,
                created_at: row.get(9)?,
                max_sources_per_run: max_sources_int as usize,
                auto_ingest: auto_ingest_int == 1,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn get_bot(&self, id: &str) -> Result<Option<ResearchBot>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let mut stmt = conn.prepare(
            "SELECT id, name, query, interval_mins, enabled, last_run_at, next_run_at, run_count, claims_found, created_at, max_sources_per_run, auto_ingest
             FROM research_bots WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            let enabled_int: i64 = row.get(4)?;
            let auto_ingest_int: i64 = row.get(11)?;
            let interval_mins_int: i64 = row.get(3)?;
            let run_count_int: i64 = row.get(7)?;
            let claims_found_int: i64 = row.get(8)?;
            let max_sources_int: i64 = row.get(10)?;
            Ok(ResearchBot {
                id: row.get(0)?,
                name: row.get(1)?,
                query: row.get(2)?,
                interval_mins: interval_mins_int as u32,
                enabled: enabled_int == 1,
                last_run_at: row.get(5)?,
                next_run_at: row.get(6)?,
                run_count: run_count_int as u64,
                claims_found: claims_found_int as u64,
                created_at: row.get(9)?,
                max_sources_per_run: max_sources_int as usize,
                auto_ingest: auto_ingest_int == 1,
            })
        })?;
        if let Some(r) = rows.next() {
            Ok(Some(r?))
        } else {
            Ok(None)
        }
    }

    pub fn update_bot(&self, bot: &ResearchBot) -> Result<(), StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        conn.execute(
            "UPDATE research_bots
             SET name = ?1, query = ?2, interval_mins = ?3, enabled = ?4, last_run_at = ?5, next_run_at = ?6, run_count = ?7, claims_found = ?8, max_sources_per_run = ?9, auto_ingest = ?10
             WHERE id = ?11",
            params![
                bot.name.trim(),
                bot.query.trim(),
                bot.interval_mins as i64,
                if bot.enabled { 1 } else { 0 },
                bot.last_run_at,
                bot.next_run_at,
                bot.run_count as i64,
                bot.claims_found as i64,
                bot.max_sources_per_run as i64,
                if bot.auto_ingest { 1 } else { 0 },
                bot.id
            ],
        )?;
        Ok(())
    }

    pub fn delete_bot(&self, id: &str) -> Result<bool, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let count = conn.execute("DELETE FROM research_bots WHERE id = ?1", params![id])?;
        Ok(count > 0)
    }

    pub fn due_bots(&self) -> Result<Vec<ResearchBot>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let mut stmt = conn.prepare(
            "SELECT id, name, query, interval_mins, enabled, last_run_at, next_run_at, run_count, claims_found, created_at, max_sources_per_run, auto_ingest
             FROM research_bots
             WHERE enabled = 1 AND (next_run_at IS NULL OR next_run_at <= ?1)
             ORDER BY next_run_at ASC",
        )?;
        let rows = stmt.query_map(params![now], |row| {
            let enabled_int: i64 = row.get(4)?;
            let auto_ingest_int: i64 = row.get(11)?;
            let interval_mins_int: i64 = row.get(3)?;
            let run_count_int: i64 = row.get(7)?;
            let claims_found_int: i64 = row.get(8)?;
            let max_sources_int: i64 = row.get(10)?;
            Ok(ResearchBot {
                id: row.get(0)?,
                name: row.get(1)?,
                query: row.get(2)?,
                interval_mins: interval_mins_int as u32,
                enabled: enabled_int == 1,
                last_run_at: row.get(5)?,
                next_run_at: row.get(6)?,
                run_count: run_count_int as u64,
                claims_found: claims_found_int as u64,
                created_at: row.get(9)?,
                max_sources_per_run: max_sources_int as usize,
                auto_ingest: auto_ingest_int == 1,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn record_bot_run(&self, run: &BotRunRecord) -> Result<(), StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        conn.execute(
            "INSERT INTO bot_runs (id, bot_id, started_at, finished_at, urls_checked, claims_added, error_msg, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                run.id,
                run.bot_id,
                run.started_at,
                run.finished_at,
                run.urls_checked as i64,
                run.claims_added as i64,
                run.error_msg,
                run.status
            ],
        )?;
        Ok(())
    }

    pub fn list_bot_runs(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<BotRunRecord>, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let mut stmt = conn.prepare(
            "SELECT id, bot_id, started_at, finished_at, urls_checked, claims_added, error_msg, status
             FROM bot_runs
             WHERE bot_id = ?1
             ORDER BY started_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![bot_id, limit as i64], |row| {
            let urls: i64 = row.get(4)?;
            let claims: i64 = row.get(5)?;
            Ok(BotRunRecord {
                id: row.get(0)?,
                bot_id: row.get(1)?,
                started_at: row.get(2)?,
                finished_at: row.get(3)?,
                urls_checked: urls as usize,
                claims_added: claims as usize,
                error_msg: row.get(6)?,
                status: row.get(7)?,
            })
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn has_document(&self, canonical_url: &str) -> Result<bool, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Lock)?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM documents WHERE canonical_url = ?1",
            params![canonical_url],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }
}

fn upsert_claim(conn: &Connection, claim: &NewClaim, now: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO claims
            (id, text, normalized, lang, kind, status, first_seen, last_seen, document_count, evidence_count)
         VALUES (?1, ?2, ?3, ?4, ?5, 'unverified', ?6, ?6, 0, 0)
         ON CONFLICT(id) DO UPDATE SET
            text = excluded.text,
            normalized = excluded.normalized,
            lang = COALESCE(excluded.lang, claims.lang),
            kind = excluded.kind,
            last_seen = excluded.last_seen",
        params![
            claim.id,
            claim.text,
            claim.normalized,
            claim.lang,
            claim.kind.as_str(),
            now,
        ],
    )?;
    Ok(())
}

fn load_claim_summary(
    conn: &Connection,
    claim_id: &str,
) -> Result<Option<ClaimSummary>, StoreError> {
    let row = conn
        .query_row(
            "SELECT id, text, kind, lang, status, document_count, evidence_count, first_seen, last_seen
             FROM claims WHERE id = ?1",
            params![claim_id],
            |row| {
                let kind: String = row.get(2)?;
                Ok(ClaimSummary {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    kind: ClaimKind::parse(&kind).unwrap_or(ClaimKind::Other),
                    lang: row.get(3)?,
                    status: row.get(4)?,
                    document_count: row.get::<_, i64>(5)? as usize,
                    evidence_count: row.get::<_, i64>(6)? as usize,
                    first_seen: row.get(7)?,
                    last_seen: row.get(8)?,
                    stances: Vec::new(),
                })
            },
        )
        .optional()?;
    let Some(mut summary) = row else {
        return Ok(None);
    };
    summary.stances = stance_counts(conn, claim_id)?;
    Ok(Some(summary))
}

fn stance_counts(conn: &Connection, claim_id: &str) -> Result<Vec<StanceCount>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT stance, COUNT(*) FROM evidence
         WHERE claim_id = ?1
         GROUP BY stance
         ORDER BY COUNT(*) DESC, stance ASC",
    )?;
    let rows = stmt.query_map(params![claim_id], |row| {
        let stance: String = row.get(0)?;
        Ok((stance, row.get::<_, i64>(1)? as usize))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (stance, count) = row?;
        if let Some(stance) = Stance::parse(&stance) {
            out.push(StanceCount { stance, count });
        }
    }
    Ok(out)
}

fn count_query(conn: &Connection, sql: &str) -> Result<usize, StoreError> {
    let count: Option<i64> = conn.query_row(sql, [], |row| row.get(0)).optional()?;
    Ok(count.unwrap_or(0) as usize)
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    declaration: &str,
) -> Result<(), StoreError> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    let mut exists = false;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            exists = true;
            break;
        }
    }
    drop(rows);
    drop(stmt);
    if !exists {
        conn.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {declaration}"
        ))?;
    }
    Ok(())
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

    fn bundle(url: &str, claim_id: &str) -> EvidenceBundle {
        let claim = bse_core::NewClaim {
            id: claim_id.to_string(),
            text: "The sky is green.".to_string(),
            normalized: "the sky is green".to_string(),
            lang: Some("en".to_string()),
            kind: bse_core::ClaimKind::Fact,
            entities: vec![bse_core::NewEntity {
                id: "sha256:entity-who".to_string(),
                kind: bse_core::EntityKind::Organization,
                name: "WHO".to_string(),
                normalized: "who".to_string(),
            }],
        };
        bse_core::EvidenceBundle {
            document: bse_core::EvidenceDocument {
                canonical_url: url.to_string(),
                title: "A source".to_string(),
                domain: "example.org".to_string(),
                excerpt: "The sky is green.".to_string(),
                content_hash: "sha256:content".to_string(),
                lang: Some("en".to_string()),
                published: None,
                fetched_at: "2026-01-01T00:00:00Z".to_string(),
            },
            claims: vec![claim],
            evidence: vec![bse_core::NewEvidence {
                claim_id: claim_id.to_string(),
                stance: Stance::Reports,
                strength: 0.9,
                quote: "The sky is green.".to_string(),
                quote_hash: "sha256:quote".to_string(),
                span_start: Some(0),
                span_end: Some(17),
            }],
            model_version: "fake-model".to_string(),
            prompt_version: "fake-prompt".to_string(),
        }
    }

    #[test]
    fn writes_and_reads_evidence_ledger() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("bald.db")).unwrap();
        let claim_id = "sha256:claim-one";
        let write = store
            .record_evidence(&bundle("https://example.org/a", claim_id))
            .unwrap();
        assert_eq!(write.claims_upserted, 1);
        assert_eq!(write.evidence_upserted, 1);
        assert_eq!(write.entities_upserted, 1);

        let detail = store.claim_detail(claim_id).unwrap().unwrap();
        assert_eq!(detail.claim.document_count, 1);
        assert_eq!(detail.claim.evidence_count, 1);
        assert_eq!(detail.evidence[0].stance, Stance::Reports);
        assert_eq!(detail.evidence[0].canonical_url, "https://example.org/a");
        assert_eq!(detail.entities[0].name, "WHO");
        assert_eq!(detail.claim.stances[0].stance, Stance::Reports);
        assert_eq!(store.ledger_stats().unwrap().claims, 1);
        assert_eq!(store.list_claims(10).unwrap().len(), 1);
        assert_eq!(
            store.resolve_claim_id("sha256:claim").unwrap().as_deref(),
            Some(claim_id)
        );
        assert_eq!(
            store.resolve_claim_id("claim-one").unwrap().as_deref(),
            Some(claim_id)
        );
    }

    #[test]
    fn reprocessing_same_quote_does_not_double_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("bald.db")).unwrap();
        let bundle = bundle("https://example.org/a", "sha256:claim-one");
        store.record_evidence(&bundle).unwrap();
        store.record_evidence(&bundle).unwrap();
        let stats = store.ledger_stats().unwrap();
        assert_eq!(stats.claims, 1);
        assert_eq!(stats.evidence, 1);
        assert_eq!(stats.entities, 1);
        assert_eq!(stats.documents_with_evidence, 1);
    }

    #[test]
    fn same_claim_from_two_documents_gets_two_sources() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("bald.db")).unwrap();
        store
            .record_evidence(&bundle("https://example.org/a", "sha256:claim-one"))
            .unwrap();
        store
            .record_evidence(&bundle("https://example.org/b", "sha256:claim-one"))
            .unwrap();
        let detail = store.claim_detail("sha256:claim-one").unwrap().unwrap();
        assert_eq!(detail.claim.document_count, 2);
        assert_eq!(detail.claim.evidence_count, 2);
        assert_eq!(detail.evidence.len(), 2);
    }

    #[test]
    fn ambiguous_prefix_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("bald.db")).unwrap();
        store
            .record_evidence(&bundle("https://example.org/a", "sha256:claim-one"))
            .unwrap();
        store
            .record_evidence(&bundle("https://example.org/b", "sha256:claim-two"))
            .unwrap();
        assert!(matches!(
            store.resolve_claim_id("sha256:claim-"),
            Err(StoreError::AmbiguousClaim(_))
        ));
    }

    #[test]
    fn research_bots_lifecycle_and_due() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("bald.db")).unwrap();

        let new_bot = NewResearchBot {
            name: "Monitor CBDC".to_string(),
            query: "cyfrowe euro cbdc".to_string(),
            interval_mins: 60,
            max_sources_per_run: Some(4),
            auto_ingest: Some(true),
        };
        let bot = store.create_bot(&new_bot).unwrap();
        assert_eq!(bot.name, "Monitor CBDC");
        assert_eq!(bot.interval_mins, 60);
        assert!(bot.enabled);
        assert_eq!(bot.run_count, 0);

        let list = store.list_bots().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, bot.id);

        let due = store.due_bots().unwrap();
        assert_eq!(due.len(), 1);

        // Record a run
        let run = BotRunRecord {
            id: Uuid::new_v4().to_string(),
            bot_id: bot.id.clone(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            finished_at: "2026-01-01T00:00:05Z".to_string(),
            urls_checked: 3,
            claims_added: 2,
            error_msg: None,
            status: "success".to_string(),
        };
        store.record_bot_run(&run).unwrap();

        let runs = store.list_bot_runs(&bot.id, 10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].claims_added, 2);

        // Update bot
        let mut updated = bot.clone();
        updated.enabled = false;
        updated.claims_found = 2;
        updated.run_count = 1;
        store.update_bot(&updated).unwrap();

        let fetched = store.get_bot(&bot.id).unwrap().unwrap();
        assert!(!fetched.enabled);
        assert_eq!(fetched.claims_found, 2);

        // Not due anymore because enabled=false
        let due_now = store.due_bots().unwrap();
        assert_eq!(due_now.len(), 0);

        // Delete bot
        assert!(store.delete_bot(&bot.id).unwrap());
        assert!(store.list_bots().unwrap().is_empty());
    }
}
