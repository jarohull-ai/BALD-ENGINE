//! Orkiestracja zapytania: guard → fan-out → kanonizacja → RRF → dedupe → store.

mod dedupe;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bse_core::{
    Gate, GateCtx, GateDecision, GateVerdict, Lang, Provider, ProviderStatus, RankedHit, RawHit,
    SearchQuery, SearchReport,
};
use bse_guard::audit::{subject_query, AuditError, AuditLog};
use bse_guard::RulesGate;
use bse_store::{RunHit, RunRecord, RunStore, StoreError};
use serde_json::json;
use thiserror::Error;
use url::Url;

use dedupe::near_dedupe;

const RRF_K: f32 = 60.0;
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("no providers configured")]
    NoProviders,

    #[error("audit error: {0}")]
    Audit(#[from] AuditError),

    #[error("store error: {0}")]
    Store(#[from] StoreError),
}

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub limit: usize,
    pub store: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: 10,
            store: true,
        }
    }
}

struct ProviderOutcome {
    status: ProviderStatus,
    hits: Vec<RawHit>,
}

pub struct SearchEngine {
    providers: Vec<Arc<dyn Provider>>,
    guard: RulesGate,
    audit: Mutex<AuditLog>,
    store: Option<Arc<dyn RunStore>>,
}

impl SearchEngine {
    pub fn new(
        providers: Vec<Arc<dyn Provider>>,
        audit: AuditLog,
        store: Option<Arc<dyn RunStore>>,
    ) -> Self {
        Self {
            providers,
            guard: RulesGate::new(),
            audit: Mutex::new(audit),
            store,
        }
    }

    pub fn provider_ids(&self) -> Vec<String> {
        self.providers.iter().map(|p| p.id().to_string()).collect()
    }

    pub async fn search(
        &self,
        raw_query: &str,
        lang: Lang,
        opts: &SearchOptions,
    ) -> Result<SearchReport, PipelineError> {
        if self.providers.is_empty() {
            return Err(PipelineError::NoProviders);
        }

        let started = Instant::now();
        let query_text = raw_query.trim().to_string();
        let subject = subject_query(&query_text);
        let mut warnings = Vec::new();

        let verdict = self.guard.check(&GateCtx::search(&query_text, lang));
        if verdict.decision != GateDecision::Allow {
            self.record_guard(&subject, &verdict, &mut warnings);
            return Ok(SearchReport {
                query: query_text,
                lang,
                gate: verdict,
                hits: Vec::new(),
                providers: Vec::new(),
                latency_ms: started.elapsed().as_millis() as u64,
                warnings,
                stored_run_id: None,
                unique_domains: 0,
                source_diversity: 0.0,
                near_duplicates_removed: 0,
            });
        }

        let search_query = SearchQuery::new(query_text.clone(), lang, opts.limit.max(1));
        let outcomes = self.fan_out(&search_query).await;

        let views: Vec<ProviderOutcomeView> = outcomes
            .iter()
            .map(|o| ProviderOutcomeView::new(o.hits.clone()))
            .collect();
        let fuse_started = Instant::now();
        let (hits, near_duplicates_removed) = fuse_with_stats(&views, opts.limit.max(1));
        let _fuse_ms = fuse_started.elapsed().as_millis();

        let provider_status: Vec<ProviderStatus> =
            outcomes.iter().map(|o| o.status.clone()).collect();

        let (unique_domains, source_diversity) = diversity(&hits);
        if hits.len() > 1 && unique_domains <= 1 {
            warnings.push("single-source: wszystkie wyniki pochodzą z jednej domeny".to_string());
        } else if hits.len() >= 3 && source_diversity < 0.6 {
            warnings.push(format!(
                "low-diversity: entropia domen {source_diversity:.2} < 0.60"
            ));
        }

        self.record_guard(&subject, &verdict, &mut warnings);

        let latency_ms = started.elapsed().as_millis() as u64;
        let mut stored_run_id = None;
        if opts.store {
            if let Some(store) = &self.store {
                let run = build_run_record(
                    &query_text,
                    &subject,
                    &provider_status,
                    &hits,
                    latency_ms,
                    false,
                );
                match store.record_run(&run) {
                    Ok(()) => stored_run_id = Some(run.id),
                    Err(e) => warnings.push(format!("store: {e}")),
                }
            }
        }

        self.record_allow(
            &subject,
            &provider_status,
            hits.len(),
            latency_ms,
            &mut warnings,
        );

        Ok(SearchReport {
            query: query_text,
            lang,
            gate: verdict,
            hits,
            providers: provider_status,
            latency_ms,
            warnings,
            stored_run_id,
            unique_domains,
            source_diversity,
            near_duplicates_removed,
        })
    }

    async fn fan_out(&self, query: &SearchQuery) -> Vec<ProviderOutcome> {
        let futures = self.providers.iter().map(|provider| {
            let provider = Arc::clone(provider);
            let query = query.clone();
            async move {
                let started = Instant::now();
                let result = tokio::time::timeout(PROVIDER_TIMEOUT, provider.search(&query)).await;
                let latency_ms = started.elapsed().as_millis() as u64;
                match result {
                    Ok(Ok(hits)) => ProviderOutcome {
                        status: ProviderStatus {
                            provider: provider.id().to_string(),
                            ok: true,
                            hits: hits.len(),
                            latency_ms,
                            error: None,
                        },
                        hits: hits
                            .into_iter()
                            .map(|mut hit| {
                                if hit.provider.trim().is_empty() {
                                    hit.provider = provider.id().to_string();
                                }
                                hit
                            })
                            .collect(),
                    },
                    Ok(Err(err)) => ProviderOutcome {
                        status: ProviderStatus {
                            provider: provider.id().to_string(),
                            ok: false,
                            hits: 0,
                            latency_ms,
                            error: Some(err.to_string()),
                        },
                        hits: Vec::new(),
                    },
                    Err(_) => ProviderOutcome {
                        status: ProviderStatus {
                            provider: provider.id().to_string(),
                            ok: false,
                            hits: 0,
                            latency_ms,
                            error: Some(format!("timeout after {}s", PROVIDER_TIMEOUT.as_secs())),
                        },
                        hits: Vec::new(),
                    },
                }
            }
        });

        futures::future::join_all(futures).await
    }

    fn record_guard(&self, subject: &str, verdict: &GateVerdict, warnings: &mut Vec<String>) {
        let Ok(mut audit) = self.audit.lock() else {
            warnings.push("audit: lock poisoned".to_string());
            return;
        };
        let decision = decision_str(verdict.decision);
        if let Err(e) = audit.append(
            "query.guard",
            subject,
            &verdict.gate,
            decision,
            json!({
                "reasons": &verdict.reasons,
                "matched": &verdict.matched,
                "score": verdict.score,
            }),
        ) {
            warnings.push(format!("audit: {e}"));
        }
    }

    fn record_allow(
        &self,
        subject: &str,
        providers: &[ProviderStatus],
        hit_count: usize,
        latency_ms: u64,
        warnings: &mut Vec<String>,
    ) {
        let Ok(mut audit) = self.audit.lock() else {
            warnings.push("audit: lock poisoned".to_string());
            return;
        };
        let providers: Vec<&str> = providers.iter().map(|p| p.provider.as_str()).collect();
        if let Err(e) = audit.append(
            "query.search",
            subject,
            "rules",
            "allow",
            json!({
                "providers": providers,
                "hits": hit_count,
                "latency_ms": latency_ms,
            }),
        ) {
            warnings.push(format!("audit: {e}"));
        }
    }
}

fn decision_str(decision: GateDecision) -> &'static str {
    match decision {
        GateDecision::Allow => "allow",
        GateDecision::Rewrite => "rewrite",
        GateDecision::Review => "review",
        GateDecision::Refuse => "refuse",
    }
}

fn build_run_record(
    query_text: &str,
    subject: &str,
    providers: &[ProviderStatus],
    hits: &[RankedHit],
    latency_ms: u64,
    refused: bool,
) -> RunRecord {
    let mut run = RunRecord::new(query_text, subject);
    run.providers = providers.iter().map(|p| p.provider.clone()).collect();
    run.hit_count = hits.len();
    run.latency_ms = latency_ms;
    run.refused = refused;
    run.hits = hits
        .iter()
        .enumerate()
        .map(|(idx, hit)| RunHit {
            canonical_url: hit.canonical_url.clone(),
            title: hit.title.clone(),
            domain: domain_of(&hit.canonical_url),
            rank: idx + 1,
            score: hit.score,
        })
        .collect();
    run
}

/// RRF (Reciprocal Rank Fusion) + dedupe po kanonicznym URL i near-duplicate.
pub fn fuse(outcomes: &[ProviderOutcomeView], limit: usize) -> Vec<RankedHit> {
    fuse_with_stats(outcomes, limit).0
}

/// Jak `fuse`, ale zwraca też liczbę usuniętych near-duplikatów.
pub fn fuse_with_stats(outcomes: &[ProviderOutcomeView], limit: usize) -> (Vec<RankedHit>, usize) {
    let mut merged: HashMap<String, RankedHit> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut seen_per_provider: HashSet<(String, String)> = HashSet::new();

    for outcome in outcomes {
        let mut hits = outcome.hits.clone();
        hits.sort_by_key(|h| h.rank);
        for hit in hits {
            let canonical_url = canonicalize_url(&hit.url).unwrap_or_else(|| hit.url.clone());
            // Ten sam URL może wystąpić dwa razy u jednego providera (np. podstrona
            // i wersja z trackingiem). Liczymy go raz na providera.
            if !seen_per_provider.insert((hit.provider.clone(), canonical_url.clone())) {
                continue;
            }
            let contribution = 1.0 / (RRF_K + hit.rank.max(1) as f32);
            let entry = merged.entry(canonical_url.clone()).or_insert_with(|| {
                order.push(canonical_url.clone());
                RankedHit {
                    title: String::new(),
                    url: canonical_url.clone(),
                    canonical_url: canonical_url.clone(),
                    snippet: String::new(),
                    score: 0.0,
                    providers: Vec::new(),
                    best_rank: hit.rank,
                    published: None,
                }
            });
            entry.score += contribution;
            if !entry.providers.contains(&hit.provider) {
                entry.providers.push(hit.provider.clone());
            }
            if hit.rank < entry.best_rank {
                entry.best_rank = hit.rank;
            }
            if entry.title.is_empty() && !hit.title.trim().is_empty() {
                entry.title = hit.title.trim().to_string();
            }
            if hit.snippet.chars().count() > entry.snippet.chars().count() {
                entry.snippet = hit.snippet.trim().to_string();
            }
            if entry.published.is_none() {
                entry.published = hit.published.clone();
            }
        }
    }

    let mut hits: Vec<RankedHit> = order
        .into_iter()
        .filter_map(|url| merged.remove(&url))
        .collect();
    for hit in &mut hits {
        if hit.title.is_empty() {
            hit.title = hit.url.clone();
        }
    }
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.best_rank.cmp(&b.best_rank))
            .then_with(|| a.title.cmp(&b.title))
    });

    let near_duplicates_removed = near_dedupe(&mut hits);
    hits.truncate(limit);
    (hits, near_duplicates_removed)
}

/// Ile unikalnych domen i jaka jest znormalizowana entropia ich rozkładu.
pub fn diversity(hits: &[RankedHit]) -> (usize, f32) {
    if hits.is_empty() {
        return (0, 0.0);
    }

    let mut counts: HashMap<String, usize> = HashMap::new();
    for hit in hits {
        let domain = domain_of(&hit.canonical_url);
        let key = if domain.is_empty() {
            hit.canonical_url.clone()
        } else {
            domain
        };
        *counts.entry(key).or_default() += 1;
    }

    let unique = counts.len();
    if unique <= 1 {
        return (unique, 0.0);
    }

    let total = hits.len() as f32;
    let entropy: f32 = counts
        .values()
        .map(|count| {
            let p = *count as f32 / total;
            -p * p.log2()
        })
        .sum();
    let max_entropy = (hits.len() as f32).log2();
    let normalized = if max_entropy > 0.0 {
        (entropy / max_entropy).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (unique, normalized)
}

/// Widok outcome'u potrzebny do fuzji (testowalny bez `SearchEngine`).
#[derive(Debug, Clone)]
pub struct ProviderOutcomeView {
    pub hits: Vec<RawHit>,
}

impl ProviderOutcomeView {
    pub fn new(hits: Vec<RawHit>) -> Self {
        Self { hits }
    }
}

/// Kanoniczny URL do dedupe: bez fragmentu, `www.`, domyślnych portów,
/// parametrów śledzących i z sortowanym query stringiem.
pub fn canonicalize_url(raw: &str) -> Option<String> {
    let mut url = Url::parse(raw.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    url.set_fragment(None);

    let host = url.host_str()?.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    url.set_host(Some(&host)).ok()?;

    let default_port = matches!(
        (url.scheme(), url.port()),
        ("http", Some(80)) | ("https", Some(443))
    );
    if default_port {
        url.set_port(None).ok()?;
    }

    let _ = url.set_username("");
    let _ = url.set_password(None);

    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| !is_tracking_param(k))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    pairs.sort();
    if pairs.is_empty() {
        url.set_query(None);
    } else {
        let mut qp = url.query_pairs_mut();
        qp.clear()
            .extend_pairs(pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    }

    let path = url.path().to_string();
    if path.len() > 1 && path.ends_with('/') {
        url.set_path(path.trim_end_matches('/'));
    }

    Some(url.to_string())
}

fn is_tracking_param(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.starts_with("utm_")
        || matches!(
            key.as_str(),
            "fbclid" | "gclid" | "yclid" | "mc_cid" | "mc_eid" | "igshid" | "ref_src"
        )
}

pub fn domain_of(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
        .map(|h| h.strip_prefix("www.").unwrap_or(&h).to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bse_providers::DemoProvider;
    use bse_store::Store;
    use std::sync::Arc;

    fn raw(provider: &str, rank: usize, url: &str, title: &str) -> RawHit {
        RawHit {
            title: title.to_string(),
            url: url.to_string(),
            snippet: format!("snippet {title}"),
            rank,
            provider: provider.to_string(),
            published: None,
        }
    }

    #[test]
    fn canonicalizes_urls() {
        assert_eq!(
            canonicalize_url("HTTPS://WWW.Example.org:443/a/?utm_source=x&b=2&a=1#frag").unwrap(),
            "https://example.org/a?a=1&b=2"
        );
        assert_eq!(
            canonicalize_url("http://example.org:80").unwrap(),
            "http://example.org/"
        );
        assert!(canonicalize_url("file:///etc/passwd").is_none());
    }

    #[test]
    fn fuses_duplicates_across_providers() {
        let a = ProviderOutcomeView::new(vec![
            raw("searxng", 1, "https://example.org/a?utm_source=x", "A"),
            raw("searxng", 2, "https://example.org/b", "B"),
        ]);
        let b = ProviderOutcomeView::new(vec![
            raw("brave", 1, "https://www.example.org/a", "A brave"),
            raw("brave", 2, "https://example.org/c", "C"),
        ]);
        let hits = fuse(&[a, b], 10);
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].canonical_url, "https://example.org/a");
        assert_eq!(hits[0].providers.len(), 2);
        assert!(hits[0].score > hits[1].score);
    }

    #[test]
    fn limit_is_respected() {
        let a = ProviderOutcomeView::new(vec![
            raw("demo", 1, "https://example.org/1", "1"),
            raw("demo", 2, "https://example.org/2", "2"),
            raw("demo", 3, "https://example.org/3", "3"),
        ]);
        assert_eq!(fuse(&[a], 2).len(), 2);
    }

    #[test]
    fn same_provider_duplicate_is_counted_once() {
        let a = ProviderOutcomeView::new(vec![
            raw("searxng", 1, "https://example.org/a?utm_source=x", "A"),
            raw("searxng", 2, "https://www.example.org/a", "A duplicate"),
            raw("searxng", 3, "https://example.org/b", "B"),
        ]);
        let hits = fuse(&[a], 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].providers, vec!["searxng"]);
        assert_eq!(hits[0].url, "https://example.org/a");
    }

    #[test]
    fn domain_of_strips_www() {
        assert_eq!(domain_of("https://www.example.org/x"), "example.org");
    }

    #[test]
    fn fuse_suppresses_near_duplicate_mirrors() {
        let text = "Cichy balon nad miastem to narracja o rzekomym obiekcie obserwowanym nocą przez wielu świadków";
        let mut a = raw("searxng", 1, "https://a.example/story", "Cichy balon");
        a.snippet = text.into();
        let mut b = raw("brave", 1, "https://b.example/mirror", "Cichy balon");
        b.snippet = text.into();

        let (hits, removed) = fuse_with_stats(
            &[
                ProviderOutcomeView::new(vec![a]),
                ProviderOutcomeView::new(vec![b]),
            ],
            10,
        );
        assert_eq!(removed, 1);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].providers, vec!["searxng", "brave"]);
    }

    #[test]
    fn diversity_is_normalized_entropy() {
        let a = ProviderOutcomeView::new(vec![
            raw("demo", 1, "https://a.example/1", "1"),
            raw("demo", 2, "https://b.example/2", "2"),
            raw("demo", 3, "https://c.example/3", "3"),
        ]);
        let hits = fuse(&[a], 10);
        let (unique, score) = diversity(&hits);
        assert_eq!(unique, 3);
        assert!(score > 0.99);

        let same = ProviderOutcomeView::new(vec![
            raw("demo", 1, "https://a.example/1", "1"),
            raw("demo", 2, "https://a.example/2", "2"),
        ]);
        let (unique, score) = diversity(&fuse(&[same], 10));
        assert_eq!(unique, 1);
        assert_eq!(score, 0.0);
    }

    #[tokio::test]
    async fn end_to_end_demo_search_is_stored_and_audited() {
        let dir = tempfile::tempdir().unwrap();
        let audit = AuditLog::open(dir.path().join("audit.jsonl")).unwrap();
        let store = Store::open(dir.path().join("bald.db")).unwrap();
        let engine = SearchEngine::new(
            vec![Arc::new(DemoProvider::new())],
            audit,
            Some(Arc::new(store)),
        );

        let report = engine
            .search(
                "teoria o cichych aktualizacjach",
                Lang::Pl,
                &SearchOptions {
                    limit: 5,
                    store: true,
                },
            )
            .await
            .unwrap();

        assert_eq!(report.gate.decision, GateDecision::Allow);
        assert_eq!(report.hits.len(), 5);
        assert!(report.stored_run_id.is_some());
        assert!(report.providers.iter().all(|p| p.ok));

        let chain = bse_guard::audit::verify_file(&dir.path().join("audit.jsonl")).unwrap();
        assert!(chain.valid);
        assert_eq!(chain.records, 2);
    }

    #[tokio::test]
    async fn refused_query_is_not_stored_and_calls_no_provider() {
        let dir = tempfile::tempdir().unwrap();
        let audit = AuditLog::open(dir.path().join("audit.jsonl")).unwrap();
        let store = Store::open(dir.path().join("bald.db")).unwrap();
        let engine = SearchEngine::new(
            vec![Arc::new(DemoProvider::new())],
            audit,
            Some(Arc::new(store)),
        );

        let report = engine
            .search(
                "Gdzie mieszka Jan Kowalski?",
                Lang::Pl,
                &SearchOptions {
                    limit: 5,
                    store: true,
                },
            )
            .await
            .unwrap();

        assert_eq!(report.gate.decision, GateDecision::Refuse);
        assert!(report.hits.is_empty());
        assert!(report.providers.is_empty());
        assert!(report.stored_run_id.is_none());
    }
}
