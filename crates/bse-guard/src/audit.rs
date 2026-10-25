use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::hash::sha256_prefixed;
use crate::normalize::normalize_for_rules;

const GENESIS: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("audit io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("audit serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("audit chain corrupted at line {line}: {message}")]
    Corrupt { line: usize, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub v: u8,
    pub seq: u64,
    pub ts: String,
    pub kind: String,
    /// SHA-256 znormalizowanego zapytania/cytatu — nigdy surowy tekst.
    pub subject: String,
    pub gate: String,
    pub decision: String,
    pub detail: Value,
    pub prev: String,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditChain {
    pub valid: bool,
    pub records: usize,
    pub first_bad: Option<usize>,
    pub error: Option<String>,
}

impl AuditChain {
    fn ok(records: usize) -> Self {
        Self {
            valid: true,
            records,
            first_bad: None,
            error: None,
        }
    }

    fn bad(records: usize, line: usize, error: impl Into<String>) -> Self {
        Self {
            valid: false,
            records,
            first_bad: Some(line),
            error: Some(error.into()),
        }
    }
}

pub struct AuditLog {
    path: PathBuf,
    file: File,
    seq: u64,
    prev: String,
}

impl AuditLog {
    /// Otwiera (lub tworzy) log. Jeśli ostatni rekord jest uszkodzony, zwraca błąd
    /// zamiast dopisywać do zerwanego łańcucha.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, AuditError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|source| AuditError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
        }

        let (seq, prev) = last_state(&path)?;
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&path).map_err(|source| AuditError::Io {
            path: path.clone(),
            source,
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }

        Ok(Self {
            path,
            file,
            seq,
            prev,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(
        &mut self,
        kind: &str,
        subject: &str,
        gate: &str,
        decision: &str,
        detail: Value,
    ) -> Result<AuditRecord, AuditError> {
        let mut record = AuditRecord {
            v: 1,
            seq: self.seq + 1,
            ts: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            kind: kind.to_string(),
            subject: subject.to_string(),
            gate: gate.to_string(),
            decision: decision.to_string(),
            detail,
            prev: self.prev.clone(),
            hash: String::new(),
        };
        record.hash = compute_hash(&record);

        let line = serde_json::to_string(&record)?;
        writeln!(self.file, "{line}").map_err(|source| AuditError::Io {
            path: self.path.clone(),
            source,
        })?;
        self.file.flush().map_err(|source| AuditError::Io {
            path: self.path.clone(),
            source,
        })?;
        let _ = self.file.sync_data();

        self.seq = record.seq;
        self.prev = record.hash.clone();
        Ok(record)
    }

    pub fn verify(&self) -> Result<AuditChain, AuditError> {
        verify_file(&self.path)
    }
}

pub fn verify_file(path: &Path) -> Result<AuditChain, AuditError> {
    if !path.exists() {
        return Ok(AuditChain::ok(0));
    }
    let file = File::open(path).map_err(|source| AuditError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let reader = BufReader::new(file);

    let mut expected_seq = 1u64;
    let mut prev = GENESIS.to_string();
    let mut count = 0usize;

    for (idx, line) in reader.lines().enumerate() {
        let line_no = idx + 1;
        let line = line.map_err(|source| AuditError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let record: AuditRecord = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                return Ok(AuditChain::bad(
                    count,
                    line_no,
                    format!("invalid json: {e}"),
                ))
            }
        };
        if record.v != 1 {
            return Ok(AuditChain::bad(
                count,
                line_no,
                format!("unsupported audit version {}", record.v),
            ));
        }
        if record.seq != expected_seq {
            return Ok(AuditChain::bad(
                count,
                line_no,
                format!("expected seq {expected_seq}, got {}", record.seq),
            ));
        }
        if record.prev != prev {
            return Ok(AuditChain::bad(
                count,
                line_no,
                "prev hash does not match previous record",
            ));
        }
        let recomputed = compute_hash(&record);
        if recomputed != record.hash {
            return Ok(AuditChain::bad(count, line_no, "record hash mismatch"));
        }
        prev = record.hash;
        expected_seq += 1;
        count += 1;
    }

    Ok(AuditChain::ok(count))
}

fn last_state(path: &Path) -> Result<(u64, String), AuditError> {
    if !path.exists() {
        return Ok((0, GENESIS.to_string()));
    }
    let file = File::open(path).map_err(|source| AuditError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let reader = BufReader::new(file);
    let mut last_line = None;
    for line in reader.lines() {
        let line = line.map_err(|source| AuditError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if !line.trim().is_empty() {
            last_line = Some(line);
        }
    }
    match last_line {
        None => Ok((0, GENESIS.to_string())),
        Some(line) => {
            let record: AuditRecord =
                serde_json::from_str(&line).map_err(|e| AuditError::Corrupt {
                    line: 0,
                    message: format!("last record is not valid json: {e}"),
                })?;
            if record.hash != compute_hash(&record) {
                return Err(AuditError::Corrupt {
                    line: 0,
                    message: "last record hash mismatch".to_string(),
                });
            }
            Ok((record.seq, record.hash))
        }
    }
}

fn compute_hash(record: &AuditRecord) -> String {
    let detail = serde_json::to_string(&record.detail).unwrap_or_else(|_| "null".to_string());
    let material = format!(
        "bald-audit-v1|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        record.v,
        record.seq,
        record.ts,
        record.kind,
        record.subject,
        record.gate,
        record.decision,
        detail,
        record.prev,
    );
    sha256_prefixed(&material)
}

/// Hash zapytania do audit logu — bez surowego tekstu.
pub fn subject_query(text: &str) -> String {
    sha256_prefixed(&normalize_for_rules(text))
}

/// Hash dowolnego tekstu (np. cytatu) do audit logu.
pub fn subject_text(text: &str) -> String {
    sha256_prefixed(text.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn temp_log() -> (tempfile::TempDir, AuditLog) {
        let dir = tempfile::tempdir().unwrap();
        let log = AuditLog::open(dir.path().join("audit.jsonl")).unwrap();
        (dir, log)
    }

    #[test]
    fn appends_and_verifies() {
        let (_dir, mut log) = temp_log();
        log.append(
            "query.guard",
            "sha256:abc",
            "rules",
            "allow",
            json!({"matched": []}),
        )
        .unwrap();
        log.append(
            "citation",
            "sha256:def",
            "citation",
            "rejected",
            json!({"score": 0.1}),
        )
        .unwrap();
        let chain = log.verify().unwrap();
        assert!(chain.valid);
        assert_eq!(chain.records, 2);
    }

    #[test]
    fn detects_tampering() {
        let (dir, mut log) = temp_log();
        log.append("query.guard", "sha256:abc", "rules", "allow", json!({}))
            .unwrap();
        log.append("query.guard", "sha256:def", "rules", "refuse", json!({}))
            .unwrap();
        let path = dir.path().join("audit.jsonl");
        let raw = std::fs::read_to_string(&path).unwrap();
        let tampered = raw.replacen("\"allow\"", "\"refuse\"", 1);
        let mut f = File::create(&path).unwrap();
        f.write_all(tampered.as_bytes()).unwrap();
        drop(f);

        let chain = verify_file(&path).unwrap();
        assert!(!chain.valid);
        assert_eq!(chain.first_bad, Some(1));
    }

    #[test]
    fn open_refuses_tampered_last_record() {
        let (dir, mut log) = temp_log();
        log.append("query.guard", "sha256:abc", "rules", "allow", json!({}))
            .unwrap();
        let path = dir.path().join("audit.jsonl");
        let raw = std::fs::read_to_string(&path).unwrap();
        let tampered = raw.replacen("\"allow\"", "\"refuse\"", 1);
        std::fs::write(&path, tampered).unwrap();

        assert!(matches!(
            AuditLog::open(&path),
            Err(AuditError::Corrupt { .. })
        ));
    }

    #[test]
    fn detects_removed_middle_record() {
        let (dir, mut log) = temp_log();
        for i in 0..3 {
            log.append(
                "query.guard",
                &format!("sha256:{i}"),
                "rules",
                "allow",
                json!({}),
            )
            .unwrap();
        }
        let path = dir.path().join("audit.jsonl");
        let raw = std::fs::read_to_string(&path).unwrap();
        let kept: Vec<&str> = raw
            .lines()
            .enumerate()
            .filter(|(i, _)| *i != 1)
            .map(|(_, l)| l)
            .collect();
        std::fs::write(&path, format!("{}\n", kept.join("\n"))).unwrap();

        let chain = verify_file(&path).unwrap();
        assert!(!chain.valid);
        assert_eq!(chain.first_bad, Some(2));
    }

    #[test]
    fn raw_query_is_not_stored() {
        let (_dir, mut log) = temp_log();
        let subject = subject_query("Gdzie mieszka Jan Kowalski?");
        log.append("query.guard", &subject, "rules", "refuse", json!({}))
            .unwrap();
        let raw = std::fs::read_to_string(log.path()).unwrap();
        assert!(!raw.contains("Kowalski"));
        assert!(raw.contains("sha256:"));
    }
}
