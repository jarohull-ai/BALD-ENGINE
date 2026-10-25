//! Lokalny indeks pełnotekstowy oparty na Tantivy (BM25).
//!
//! Indeks nie jest źródłem prawdy — dokumenty i relacje będą w SQLite. Tutaj
//! trzymamy tylko to, co potrzebne do wyszukiwania i prezentacji snippetu.
//! Pełna treść jest indeksowana, ale nie przechowywana; przechowujemy excerpt.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tantivy::collector::TopDocs;
use tantivy::query::{QueryParser, QueryParserError};
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TantivyDocument, TextFieldIndexing, TextOptions, Value,
    STORED, STRING,
};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyError, Term};
use thiserror::Error;

const INDEX_WRITER_BYTES: usize = 50_000_000;
const EXCERPT_CHARS: usize = 360;

#[derive(Debug, Error)]
pub enum IndexError {
    #[error("tantivy error: {0}")]
    Tantivy(#[from] TantivyError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("query parse error: {0}")]
    Query(String),

    #[error("empty query")]
    EmptyQuery,

    #[error("index writer lock poisoned")]
    Lock,

    #[error("index schema is missing field `{0}`")]
    MissingField(&'static str),
}

impl From<QueryParserError> for IndexError {
    fn from(value: QueryParserError) -> Self {
        IndexError::Query(value.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDocument {
    pub id: String,
    pub canonical_url: String,
    pub title: String,
    pub body: String,
    pub excerpt: String,
    pub domain: String,
    pub published: String,
    pub lang: String,
    pub fetched_at: String,
    pub content_hash: String,
    pub html_hash: String,
}

impl IndexDocument {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        canonical_url: impl Into<String>,
        title: impl Into<String>,
        body: impl Into<String>,
        description: impl Into<String>,
        domain: impl Into<String>,
        published: Option<String>,
        lang: Option<String>,
        fetched_at: impl Into<String>,
        content_hash: impl Into<String>,
        html_hash: impl Into<String>,
    ) -> Self {
        let canonical_url = canonical_url.into();
        let body = body.into();
        let description = description.into();
        let excerpt = if description.trim().is_empty() {
            truncate_chars(&body, EXCERPT_CHARS)
        } else {
            truncate_chars(&description, EXCERPT_CHARS)
        };

        Self {
            id: document_id(&canonical_url),
            canonical_url,
            title: title.into(),
            body,
            excerpt,
            domain: domain.into(),
            published: published.unwrap_or_default(),
            lang: lang.unwrap_or_default(),
            fetched_at: fetched_at.into(),
            content_hash: content_hash.into(),
            html_hash: html_hash.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedHit {
    pub score: f32,
    pub title: String,
    pub url: String,
    pub domain: String,
    pub excerpt: String,
    pub published: Option<String>,
    pub lang: Option<String>,
    pub fetched_at: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IndexStats {
    pub documents: u64,
}

struct Fields {
    id: Field,
    canonical_url: Field,
    title: Field,
    body: Field,
    excerpt: Field,
    domain: Field,
    published: Field,
    lang: Field,
    fetched_at: Field,
    content_hash: Field,
    html_hash: Field,
}

pub struct DocumentIndex {
    path: PathBuf,
    index: Index,
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    fields: Fields,
}

impl DocumentIndex {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, IndexError> {
        let path = path.as_ref().to_path_buf();
        fs::create_dir_all(&path)?;
        set_private_dir(&path);

        let index = if path.join("meta.json").exists() {
            Index::open_in_dir(&path)?
        } else {
            Index::create_in_dir(&path, build_schema())?
        };

        let schema = index.schema();
        let fields = Fields {
            id: field(&schema, "id")?,
            canonical_url: field(&schema, "canonical_url")?,
            title: field(&schema, "title")?,
            body: field(&schema, "body")?,
            excerpt: field(&schema, "excerpt")?,
            domain: field(&schema, "domain")?,
            published: field(&schema, "published")?,
            lang: field(&schema, "lang")?,
            fetched_at: field(&schema, "fetched_at")?,
            content_hash: field(&schema, "content_hash")?,
            html_hash: field(&schema, "html_hash")?,
        };

        let writer = index.writer(INDEX_WRITER_BYTES)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;

        Ok(Self {
            path,
            index,
            reader,
            writer: Mutex::new(writer),
            fields,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Upsert po deterministycznym `id` (SHA-256 kanonicznego URL).
    pub fn add(&self, document: &IndexDocument) -> Result<(), IndexError> {
        self.add_many(std::slice::from_ref(document))
    }

    pub fn add_many(&self, documents: &[IndexDocument]) -> Result<(), IndexError> {
        if documents.is_empty() {
            return Ok(());
        }

        let mut writer = self.writer.lock().map_err(|_| IndexError::Lock)?;
        for document in documents {
            writer.delete_term(Term::from_field_text(self.fields.id, &document.id));
            writer.add_document(self.to_tantivy(document))?;
        }
        writer.commit()?;
        self.reader.reload()?;
        harden_index_files(&self.path);
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<IndexedHit>, IndexError> {
        if query.trim().is_empty() {
            return Err(IndexError::EmptyQuery);
        }

        let parser = QueryParser::for_index(&self.index, vec![self.fields.title, self.fields.body]);
        let parsed = parser.parse_query(query)?;
        let searcher = self.reader.searcher();
        let top = searcher.search(&parsed, &TopDocs::with_limit(limit.max(1)).order_by_score())?;

        let mut hits = Vec::with_capacity(top.len());
        for (score, address) in top {
            let document: TantivyDocument = searcher.doc(address)?;
            hits.push(IndexedHit {
                score,
                title: text(&document, self.fields.title),
                url: text(&document, self.fields.canonical_url),
                domain: text(&document, self.fields.domain),
                excerpt: text(&document, self.fields.excerpt),
                published: optional_text(&document, self.fields.published),
                lang: optional_text(&document, self.fields.lang),
                fetched_at: text(&document, self.fields.fetched_at),
                content_hash: text(&document, self.fields.content_hash),
            });
        }
        Ok(hits)
    }

    pub fn stats(&self) -> Result<IndexStats, IndexError> {
        let searcher = self.reader.searcher();
        Ok(IndexStats {
            documents: searcher.num_docs(),
        })
    }

    fn to_tantivy(&self, document: &IndexDocument) -> TantivyDocument {
        let mut doc = TantivyDocument::default();
        doc.add_text(self.fields.id, &document.id);
        doc.add_text(self.fields.canonical_url, &document.canonical_url);
        doc.add_text(self.fields.title, &document.title);
        doc.add_text(self.fields.body, &document.body);
        doc.add_text(self.fields.excerpt, &document.excerpt);
        doc.add_text(self.fields.domain, &document.domain);
        doc.add_text(self.fields.published, &document.published);
        doc.add_text(self.fields.lang, &document.lang);
        doc.add_text(self.fields.fetched_at, &document.fetched_at);
        doc.add_text(self.fields.content_hash, &document.content_hash);
        doc.add_text(self.fields.html_hash, &document.html_hash);
        doc
    }
}

fn build_schema() -> Schema {
    let mut builder = Schema::builder();
    let text = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("default")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );

    builder.add_text_field("id", STRING | STORED);
    builder.add_text_field("canonical_url", STRING | STORED);
    builder.add_text_field("title", text.clone() | STORED);
    builder.add_text_field("body", text);
    builder.add_text_field("excerpt", TextOptions::default().set_stored());
    builder.add_text_field("domain", STRING | STORED);
    builder.add_text_field("published", STRING | STORED);
    builder.add_text_field("lang", STRING | STORED);
    builder.add_text_field("fetched_at", STRING | STORED);
    builder.add_text_field("content_hash", STRING | STORED);
    builder.add_text_field("html_hash", STRING | STORED);
    builder.build()
}

fn field(schema: &Schema, name: &'static str) -> Result<Field, IndexError> {
    schema
        .get_field(name)
        .map_err(|_| IndexError::MissingField(name))
}

fn text(document: &TantivyDocument, field: Field) -> String {
    document
        .get_first(field)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string()
}

fn optional_text(document: &TantivyDocument, field: Field) -> Option<String> {
    let value = text(document, field);
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn document_id(canonical_url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(canonical_url.as_bytes());
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        value.chars().take(max).collect()
    }
}

fn set_private_dir(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
    }
}

fn harden_index_files(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            if file_type.is_file() {
                let _ = fs::set_permissions(entry.path(), fs::Permissions::from_mode(0o600));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(id_url: &str, title: &str, body: &str) -> IndexDocument {
        IndexDocument::new(
            id_url,
            title,
            body,
            format!("Opis: {title}"),
            "example.com",
            Some("2024-01-02".into()),
            Some("pl".into()),
            "2026-01-01T00:00:00.000Z",
            "sha256:content",
            "sha256:html",
        )
    }

    #[test]
    fn add_search_and_upsert() {
        let dir = tempfile::tempdir().unwrap();
        let index = DocumentIndex::open(dir.path()).unwrap();
        index
            .add(&doc(
                "https://example.com/balon",
                "Cichy balon",
                "Analiza rzekomego cichego balonu nad miastem.",
            ))
            .unwrap();

        let hits = index.search("balon", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Cichy balon");
        assert_eq!(hits[0].url, "https://example.com/balon");

        index
            .add(&doc(
                "https://example.com/balon",
                "Cichy balon — korekta",
                "Skorygowana analiza balonu.",
            ))
            .unwrap();
        assert_eq!(index.stats().unwrap().documents, 1);
        let hits = index.search("korekta", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Cichy balon — korekta");
    }

    #[test]
    fn searches_title_and_body_and_returns_empty_for_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let index = DocumentIndex::open(dir.path()).unwrap();
        index
            .add_many(&[
                doc(
                    "https://example.com/a",
                    "Raport wodny",
                    "Treść o jakości wody.",
                ),
                doc(
                    "https://example.com/b",
                    "Inny dokument",
                    "Treść o balonach.",
                ),
            ])
            .unwrap();

        assert_eq!(index.search("wodny", 10).unwrap().len(), 1);
        assert_eq!(index.search("balonach", 10).unwrap().len(), 1);
        assert!(index.search("nieistniejące", 10).unwrap().is_empty());
        assert_eq!(index.stats().unwrap().documents, 2);
    }

    #[test]
    fn rejects_empty_query() {
        let dir = tempfile::tempdir().unwrap();
        let index = DocumentIndex::open(dir.path()).unwrap();
        assert!(matches!(
            index.search("  ", 10),
            Err(IndexError::EmptyQuery)
        ));
    }

    #[test]
    fn stable_document_id() {
        assert_eq!(
            document_id("https://example.com/a"),
            document_id("https://example.com/a")
        );
        assert_ne!(
            document_id("https://example.com/a"),
            document_id("https://example.com/b")
        );
    }
}
