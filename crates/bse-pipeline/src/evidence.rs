//! Łańcuch dowodowy: fetch → classify → CitationGate → evidence ledger.
//!
//! Klasyfikator może się mylić. Dlatego do ledgera trafiają wyłącznie
//! twierdzenia z cytatem, który `CitationGate` potwierdził w tekście źródła.
//! Odrzucone twierdzenia są raportowane, ale nie zapisywane.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use bse_classify::{ClassifiedEntity, Classifier, ClassifyError, ClassifyInput};
use bse_core::{
    ClaimKind, EntityKind, EvidenceBundle, EvidenceDocument, Lang, NewClaim, NewEntity,
    NewEvidence, Stance,
};
use bse_fetch::{FetchError, FetchedDocument, Fetcher};
use bse_guard::audit::AuditError;
use bse_guard::{
    normalize_for_match, sha256_prefixed, AuditLog, CitationGate, CitationStatus, Span,
};
use bse_store::{Store, StoreError};
use serde::Serialize;
use serde_json::json;
use thiserror::Error;

use crate::domain_of;

/// Krótszy fragment (np. „The Great Reset”) bywa poprawny leksykalnie, ale nie
/// dokumentuje twierdzenia. Taki cytat nie trafia do ledgera, nawet jeśli
/// `CitationGate` znalazłby go w źródle.
const MIN_EVIDENCE_QUOTE_CHARS: usize = 24;

#[derive(Debug, Error)]
pub enum EvidencePipelineError {
    #[error("fetch failed: {0}")]
    Fetch(#[from] FetchError),

    #[error("classification failed: {0}")]
    Classify(#[from] ClassifyError),

    #[error("evidence store failed: {0}")]
    Store(#[from] StoreError),

    #[error("audit failed: {0}")]
    Audit(#[from] AuditError),

    #[error("source extracted text is empty")]
    EmptyText,
}

#[derive(Debug, Clone)]
pub struct EvidenceOptions {
    /// `noindex` oznacza „nie indeksuj”. Lokalny evidence ledger też jest
    /// indeksem, więc domyślnie respektujemy tę dyrektywę.
    pub skip_noindex: bool,
}

impl Default for EvidenceOptions {
    fn default() -> Self {
        Self { skip_noindex: true }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AcceptedEvidence {
    pub claim_id: String,
    pub text: String,
    pub kind: ClaimKind,
    pub stance: Stance,
    pub confidence: f32,
    pub quote: String,
    pub quote_hash: String,
    pub citation_status: CitationStatus,
    pub citation_score: f32,
    pub span: Option<Span>,
    pub entities: Vec<ClassifiedEntity>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RejectedEvidence {
    pub claim_hash: String,
    pub text: String,
    pub reason: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceReport {
    pub requested_url: String,
    pub canonical_url: String,
    pub title: String,
    pub domain: String,
    pub content_hash: String,
    pub model: String,
    pub prompt_version: String,
    pub input_chars: usize,
    pub truncated: bool,
    pub prompt_injection_suspected: bool,
    pub skipped: Option<String>,
    pub accepted: Vec<AcceptedEvidence>,
    pub rejected: Vec<RejectedEvidence>,
    pub warnings: Vec<String>,
    pub document_id: Option<String>,
    pub claims_stored: usize,
    pub evidence_stored: usize,
    pub entities_stored: usize,
    pub latency_ms: u64,
}

pub struct EvidencePipeline {
    fetcher: Fetcher,
    classifier: Arc<dyn Classifier>,
    store: Arc<Store>,
    audit: Mutex<AuditLog>,
    gate: CitationGate,
    options: EvidenceOptions,
}

impl EvidencePipeline {
    pub fn new(
        fetcher: Fetcher,
        classifier: Arc<dyn Classifier>,
        store: Arc<Store>,
        audit: AuditLog,
        options: EvidenceOptions,
    ) -> Self {
        Self {
            fetcher,
            classifier,
            store,
            audit: Mutex::new(audit),
            gate: CitationGate::new(),
            options,
        }
    }

    pub async fn ingest(
        &self,
        raw_url: &str,
        lang: Lang,
    ) -> Result<EvidenceReport, EvidencePipelineError> {
        let subject = sha256_prefixed(raw_url.trim());
        let document = match self.fetcher.fetch(raw_url).await {
            Ok(document) => document,
            Err(error) => {
                let mut warnings = Vec::new();
                self.audit(
                    "evidence.fetch",
                    &subject,
                    "fetch",
                    "refuse",
                    json!({"kind": error.kind()}),
                    &mut warnings,
                );
                return Err(error.into());
            }
        };
        let mut warnings = Vec::new();
        self.audit(
            "evidence.fetch",
            &subject,
            "fetch",
            "allow",
            json!({
                "status": document.status,
                "cache_hit": document.cache_hit,
                "noindex": document.noindex,
                "content_hash": document.content_hash,
                "content_type": document.content_type,
            }),
            &mut warnings,
        );
        let mut report = self.ingest_document(document, lang).await?;
        report.warnings.splice(0..0, warnings);
        Ok(report)
    }

    /// Wejście bez sieci — używane przez testy i przyszłe importy archiwalne.
    pub async fn ingest_document(
        &self,
        document: FetchedDocument,
        lang: Lang,
    ) -> Result<EvidenceReport, EvidencePipelineError> {
        let started = Instant::now();
        let subject = sha256_prefixed(&document.canonical_url);
        let domain = domain_of(&document.canonical_url);

        if document.noindex && self.options.skip_noindex {
            let mut warnings = vec!["skipped_noindex".to_string()];
            self.audit(
                "evidence.ingest",
                &subject,
                "citation",
                "skip",
                json!({"reason": "noindex", "content_hash": document.content_hash}),
                &mut warnings,
            );
            return Ok(EvidenceReport {
                requested_url: document.requested_url,
                canonical_url: document.canonical_url,
                title: document.title,
                domain,
                content_hash: document.content_hash,
                model: String::new(),
                prompt_version: String::new(),
                input_chars: 0,
                truncated: false,
                prompt_injection_suspected: false,
                skipped: Some("noindex".to_string()),
                accepted: Vec::new(),
                rejected: Vec::new(),
                warnings,
                document_id: None,
                claims_stored: 0,
                evidence_stored: 0,
                entities_stored: 0,
                latency_ms: started.elapsed().as_millis() as u64,
            });
        }

        if document.text.trim().is_empty() {
            return Err(EvidencePipelineError::EmptyText);
        }

        let lang_code = document
            .lang
            .clone()
            .or_else(|| lang.code().map(str::to_string));
        let input = ClassifyInput::new(
            document.title.clone(),
            document.canonical_url.clone(),
            document.text.clone(),
            lang_code.clone(),
        );

        let classification = match self.classifier.classify(&input).await {
            Ok(classification) => classification,
            Err(error) => {
                let mut warnings = Vec::new();
                self.audit(
                    "evidence.ingest",
                    &subject,
                    "classify",
                    "refuse",
                    json!({"kind": error.kind(), "content_hash": document.content_hash}),
                    &mut warnings,
                );
                return Err(error.into());
            }
        };

        let mut warnings = classification.warnings.clone();
        let mut accepted = Vec::new();
        let mut rejected = Vec::new();

        for claim in &classification.claims {
            let claim_hash = sha256_prefixed(&normalize_for_match(&claim.text));
            let quote_chars = claim.quote.trim().chars().count();
            if quote_chars < MIN_EVIDENCE_QUOTE_CHARS {
                self.audit(
                    "evidence.quote",
                    &claim_hash,
                    "citation",
                    "refuse",
                    json!({
                        "reason": "quote_too_short",
                        "quote_chars": quote_chars,
                        "quote_hash": sha256_prefixed(&normalize_for_match(&claim.quote)),
                    }),
                    &mut warnings,
                );
                rejected.push(RejectedEvidence {
                    claim_hash,
                    text: claim.text.clone(),
                    reason: "quote_too_short".to_string(),
                    score: 0.0,
                });
                continue;
            }
            let verdict = self.gate.verify(&claim.text, &claim.quote, &document.text);
            let status = citation_status_str(verdict.status);
            self.audit(
                "evidence.citation",
                &claim_hash,
                "citation",
                if verdict.is_verified() {
                    "allow"
                } else {
                    "refuse"
                },
                json!({
                    "status": status,
                    "score": verdict.score,
                    "quote_hash": verdict.quote_hash,
                    "stance": claim.stance.as_str(),
                    "accepted": verdict.is_verified(),
                }),
                &mut warnings,
            );

            if !verdict.is_verified() {
                rejected.push(RejectedEvidence {
                    claim_hash,
                    text: claim.text.clone(),
                    reason: format!("citation:{status}"),
                    score: verdict.score,
                });
                continue;
            }

            let normalized = normalize_for_match(&claim.text);
            if normalized.is_empty() {
                rejected.push(RejectedEvidence {
                    claim_hash,
                    text: claim.text.clone(),
                    reason: "citation:empty_normalized_claim".to_string(),
                    score: verdict.score,
                });
                continue;
            }
            let claim_id = sha256_prefixed(&format!("claim:{normalized}"));
            accepted.push(AcceptedEvidence {
                claim_id,
                text: claim.text.clone(),
                kind: claim.kind,
                stance: claim.stance,
                confidence: claim.confidence,
                quote: claim.quote.clone(),
                quote_hash: verdict.quote_hash.clone(),
                citation_status: verdict.status,
                citation_score: verdict.score,
                span: verdict.span,
                entities: claim.entities.clone(),
            });
        }

        let write = if accepted.is_empty() {
            None
        } else {
            let bundle = build_bundle(
                &document,
                &domain,
                lang_code,
                &classification.model,
                &classification.prompt_version,
                &accepted,
            );
            match self.store.record_evidence(&bundle) {
                Ok(write) => Some(write),
                Err(error) => {
                    self.audit(
                        "evidence.ingest",
                        &subject,
                        "store",
                        "refuse",
                        json!({"kind": "store", "content_hash": document.content_hash}),
                        &mut warnings,
                    );
                    return Err(error.into());
                }
            }
        };

        let claims_stored = write
            .as_ref()
            .map(|write| write.claims_upserted)
            .unwrap_or(0);
        let evidence_stored = write
            .as_ref()
            .map(|write| write.evidence_upserted)
            .unwrap_or(0);
        let entities_stored = write
            .as_ref()
            .map(|write| write.entities_upserted)
            .unwrap_or(0);

        self.audit(
            "evidence.ingest",
            &subject,
            "citation",
            "allow",
            json!({
                "content_hash": document.content_hash,
                "model": classification.model,
                "prompt_version": classification.prompt_version,
                "claims_accepted": accepted.len(),
                "claims_rejected": rejected.len(),
                "claims_stored": claims_stored,
                "evidence_stored": evidence_stored,
                "prompt_injection_suspected": classification.prompt_injection_suspected,
            }),
            &mut warnings,
        );

        Ok(EvidenceReport {
            requested_url: document.requested_url,
            canonical_url: document.canonical_url,
            title: document.title,
            domain,
            content_hash: document.content_hash,
            model: classification.model,
            prompt_version: classification.prompt_version,
            input_chars: classification.input_chars,
            truncated: classification.truncated,
            prompt_injection_suspected: classification.prompt_injection_suspected,
            skipped: None,
            accepted,
            rejected,
            warnings,
            document_id: write.as_ref().map(|write| write.document_id.clone()),
            claims_stored,
            evidence_stored,
            entities_stored,
            latency_ms: started.elapsed().as_millis() as u64,
        })
    }

    fn audit(
        &self,
        kind: &str,
        subject: &str,
        gate: &str,
        decision: &str,
        detail: serde_json::Value,
        warnings: &mut Vec<String>,
    ) {
        let Ok(mut audit) = self.audit.lock() else {
            warnings.push("audit: lock poisoned".to_string());
            return;
        };
        if let Err(error) = audit.append(kind, subject, gate, decision, detail) {
            warnings.push(format!("audit: {error}"));
        }
    }
}

fn build_bundle(
    document: &FetchedDocument,
    domain: &str,
    lang: Option<String>,
    model: &str,
    prompt_version: &str,
    accepted: &[AcceptedEvidence],
) -> EvidenceBundle {
    let claims = accepted
        .iter()
        .map(|item| NewClaim {
            id: item.claim_id.clone(),
            text: item.text.clone(),
            normalized: normalize_for_match(&item.text),
            lang: lang.clone(),
            kind: item.kind,
            entities: item
                .entities
                .iter()
                .map(|entity| NewEntity {
                    id: entity_id(entity.kind, &entity.name),
                    kind: entity.kind,
                    name: entity.name.clone(),
                    normalized: normalize_for_match(&entity.name),
                })
                .collect(),
        })
        .collect::<Vec<_>>();

    let evidence = accepted
        .iter()
        .map(|item| NewEvidence {
            claim_id: item.claim_id.clone(),
            stance: item.stance,
            strength: item.confidence,
            quote: item.quote.clone(),
            quote_hash: item.quote_hash.clone(),
            span_start: item.span.map(|span| span.start as u32),
            span_end: item.span.map(|span| span.end as u32),
        })
        .collect::<Vec<_>>();

    EvidenceBundle {
        document: EvidenceDocument {
            canonical_url: document.canonical_url.clone(),
            title: document.title.clone(),
            domain: domain.to_string(),
            excerpt: if document.description.trim().is_empty() {
                document.text.chars().take(320).collect()
            } else {
                document.description.clone()
            },
            content_hash: document.content_hash.clone(),
            lang,
            published: document.published.clone(),
            fetched_at: document.fetched_at.clone(),
        },
        claims,
        evidence,
        model_version: model.to_string(),
        prompt_version: prompt_version.to_string(),
    }
}

fn entity_id(kind: EntityKind, name: &str) -> String {
    sha256_prefixed(&format!(
        "entity:{}:{}",
        kind.as_str(),
        normalize_for_match(name)
    ))
}

fn citation_status_str(status: CitationStatus) -> &'static str {
    match status {
        CitationStatus::VerifiedExact => "verified_exact",
        CitationStatus::VerifiedNormalized => "verified_normalized",
        CitationStatus::Rejected => "rejected",
        CitationStatus::SourceTooShort => "source_too_short",
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use async_trait::async_trait;
    use bse_classify::{Classification, Classifier, ClassifyError, ClassifyInput};
    use bse_core::{ClaimKind, EntityKind, Stance};
    use bse_fetch::FetchOptions;

    use super::*;

    struct FakeClassifier {
        claims: Vec<bse_classify::ClassifiedClaim>,
    }

    #[async_trait]
    impl Classifier for FakeClassifier {
        async fn classify(&self, _input: &ClassifyInput) -> Result<Classification, ClassifyError> {
            Ok(Classification {
                claims: self.claims.clone(),
                model: "fake-model".to_string(),
                prompt_version: "fake-prompt".to_string(),
                input_chars: 42,
                truncated: false,
                prompt_injection_suspected: false,
                warnings: vec!["fake".to_string()],
            })
        }
    }

    fn document(text: &str, noindex: bool) -> FetchedDocument {
        FetchedDocument {
            requested_url: "https://example.org/a".to_string(),
            final_url: "https://example.org/a".to_string(),
            canonical_url: "https://example.org/a".to_string(),
            status: 200,
            content_type: "text/html".to_string(),
            title: "A source".to_string(),
            description: "description".to_string(),
            text: text.to_string(),
            lang: Some("en".to_string()),
            published: None,
            content_hash: "sha256:content".to_string(),
            html_hash: "sha256:html".to_string(),
            fetched_at: "2026-01-01T00:00:00Z".to_string(),
            noindex,
            cache_hit: false,
        }
    }

    fn claim(text: &str, quote: &str) -> bse_classify::ClassifiedClaim {
        bse_classify::ClassifiedClaim {
            text: text.to_string(),
            quote: quote.to_string(),
            stance: Stance::Reports,
            kind: ClaimKind::Fact,
            confidence: 0.9,
            entities: vec![bse_classify::ClassifiedEntity {
                name: "WHO".to_string(),
                kind: EntityKind::Organization,
            }],
        }
    }

    fn pipeline(
        dir: &Path,
        claims: Vec<bse_classify::ClassifiedClaim>,
    ) -> (EvidencePipeline, Arc<Store>) {
        let store = Arc::new(Store::open(dir.join("bald.db")).unwrap());
        let audit = AuditLog::open(dir.join("audit.jsonl")).unwrap();
        let fetcher = Fetcher::new(FetchOptions::default()).unwrap();
        let classifier = Arc::new(FakeClassifier { claims });
        let pipeline = EvidencePipeline::new(
            fetcher,
            classifier,
            Arc::clone(&store),
            audit,
            EvidenceOptions::default(),
        );
        (pipeline, store)
    }

    #[tokio::test]
    async fn accepted_claim_reaches_ledger() {
        let dir = tempfile::tempdir().unwrap();
        let text = "The sky is green according to the report.";
        let (pipeline, store) = pipeline(dir.path(), vec![claim(text, text)]);
        let report = pipeline
            .ingest_document(document(text, false), Lang::En)
            .await
            .unwrap();
        assert_eq!(report.accepted.len(), 1);
        assert_eq!(report.rejected.len(), 0);
        assert_eq!(report.claims_stored, 1);
        assert_eq!(report.evidence_stored, 1);
        assert_eq!(report.entities_stored, 1);

        let claim_id = &report.accepted[0].claim_id;
        let detail = store.claim_detail(claim_id).unwrap().unwrap();
        assert_eq!(detail.claim.document_count, 1);
        assert_eq!(detail.entities[0].name, "WHO");

        let audit_text = std::fs::read_to_string(dir.path().join("audit.jsonl")).unwrap();
        assert!(!audit_text.contains(text));
        assert!(audit_text.contains("evidence.citation"));
        assert!(audit_text.contains("evidence.ingest"));
        let chain = AuditLog::open(dir.path().join("audit.jsonl"))
            .unwrap()
            .verify()
            .unwrap();
        assert!(chain.valid);
    }

    #[tokio::test]
    async fn quote_not_in_source_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let (pipeline, store) = pipeline(
            dir.path(),
            vec![claim(
                "The sky is green.",
                "This quote is not in the source.",
            )],
        );
        let report = pipeline
            .ingest_document(
                document(
                    "The sky is green according to the report, but the quote below is absent.",
                    false,
                ),
                Lang::En,
            )
            .await
            .unwrap();
        assert!(report.accepted.is_empty());
        assert_eq!(report.rejected.len(), 1);
        assert_eq!(report.rejected[0].reason, "citation:rejected");
        assert_eq!(store.ledger_stats().unwrap().claims, 0);
        assert_eq!(store.ledger_stats().unwrap().evidence, 0);
    }

    #[tokio::test]
    async fn short_quote_is_rejected_even_if_it_exists_in_source() {
        let dir = tempfile::tempdir().unwrap();
        let (pipeline, store) = pipeline(
            dir.path(),
            vec![claim(
                "The Great Reset was chosen as the theme of the 2021 WEF meeting.",
                "The Great Reset",
            )],
        );
        let report = pipeline
            .ingest_document(
                document(
                    "The Great Reset was chosen as the theme of the 2021 WEF meeting.",
                    false,
                ),
                Lang::En,
            )
            .await
            .unwrap();
        assert!(report.accepted.is_empty());
        assert_eq!(report.rejected.len(), 1);
        assert_eq!(report.rejected[0].reason, "quote_too_short");
        assert_eq!(store.ledger_stats().unwrap().claims, 0);
        let audit_text = std::fs::read_to_string(dir.path().join("audit.jsonl")).unwrap();
        assert!(audit_text.contains("evidence.quote"));
        assert!(!audit_text.contains("The Great Reset"));
    }

    #[tokio::test]
    async fn noindex_is_skipped_before_llm() {
        let dir = tempfile::tempdir().unwrap();
        let (pipeline, store) = pipeline(
            dir.path(),
            vec![claim("The sky is green.", "The sky is green.")],
        );
        let report = pipeline
            .ingest_document(document("The sky is green.", true), Lang::En)
            .await
            .unwrap();
        assert_eq!(report.skipped.as_deref(), Some("noindex"));
        assert!(report.accepted.is_empty());
        assert_eq!(store.ledger_stats().unwrap().claims, 0);
    }
}
