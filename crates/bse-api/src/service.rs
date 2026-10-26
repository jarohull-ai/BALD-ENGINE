//! Wspólna warstwa usług dla `bald serve` i komend Tauri.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use bse_classify::{
    Classifier, ClassifyOptions, FallbackClassifier, HeuristicClassifier, LlmClassifier,
};
use bse_config::{config_path, expand_tilde, Config, ConfigError};
use bse_core::{Gate, GateCtx, GateDecision, GatePurpose, Lang, Provider};
use bse_fetch::{FetchCache, FetchOptions, Fetcher};
use bse_guard::{AuditLog, RulesGate};
use bse_llm::{Llm, LlmClient, LlmProvider, LlmSettings};
use bse_pipeline::{domain_of, EvidenceOptions, EvidencePipeline, SearchEngine, SearchOptions};
use bse_providers::{
    BraveProvider, DemoProvider, DuckDuckGoProvider, SearxngProvider, WikipediaProvider,
};
use bse_store::{
    BotRunRecord, ClaimDetail, ClaimSummary, LedgerStats, NewResearchBot, ResearchBot, RunStore,
    Store, StoreError,
};

use crate::settings::{self, SettingsError, SettingsUpdate, SettingsView};

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("cannot load config: {0}")]
    Config(#[from] ConfigError),

    #[error("cannot open evidence store: {0}")]
    Store(#[from] StoreError),

    #[error(transparent)]
    Settings(#[from] SettingsError),

    #[error("claim not found: {0}")]
    NotFound(String),

    #[error("pipeline error: {0}")]
    Pipeline(String),

    #[error("fetch error: {0}")]
    Fetch(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("service state lock poisoned")]
    Lock,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    pub history: Option<Vec<ChatMessage>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptedClaimInfo {
    pub claim_id: String,
    pub text: String,
    pub kind: String,
    pub stance: String,
    pub quote: String,
    pub quote_hash: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestResponse {
    pub url: String,
    pub title: String,
    pub domain: String,
    pub claims_found: usize,
    pub claims_accepted: usize,
    pub claims_rejected: usize,
    pub accepted: Vec<AcceptedClaimInfo>,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSource {
    pub title: String,
    pub url: String,
    pub domain: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub role: String,
    pub content: String,
    pub is_url: bool,
    pub guard_refused: bool,
    pub ingest: Option<IngestResponse>,
    pub matching_claims: Vec<ClaimSummary>,
    #[serde(default)]
    pub sources: Vec<ChatSource>,
    #[serde(default)]
    pub auto_researched: bool,
    #[serde(default)]
    pub new_claims_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub domain: String,
    pub snippet: String,
    pub score: f32,
    pub providers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchWebResponse {
    pub query: String,
    pub hits: Vec<SearchHit>,
    pub guard_refused: bool,
    pub guard_message: Option<String>,
    pub total: usize,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaimsResponse {
    pub claims: Vec<ClaimSummary>,
    pub stats: LedgerStats,
    pub generated_at: String,
}

pub struct Service {
    store: Arc<Store>,
    config_path: PathBuf,
    config: RwLock<Config>,
}

impl Service {
    pub fn open() -> Result<Self, ServiceError> {
        Self::open_at(config_path())
    }

    /// Otwiera usługę z jawną ścieżką konfiguracji. Przydatne w testach i
    /// w trybie portable; runtime korzysta z `Config::load` (env nadpisuje plik).
    pub fn open_at(path: PathBuf) -> Result<Self, ServiceError> {
        let file_config = Config::load_from(&path)?;
        let mut effective = file_config.clone();
        effective.apply_env();
        let store = Store::open(expand_tilde(&effective.storage.path))?;
        Ok(Self {
            store: Arc::new(store),
            config_path: path,
            config: RwLock::new(file_config),
        })
    }

    pub fn claims(&self, limit: usize) -> Result<ClaimsResponse, ServiceError> {
        let claims = self.store.list_claims(limit.clamp(1, 500))?;
        let stats = self.store.ledger_stats()?;
        Ok(ClaimsResponse {
            claims,
            stats,
            generated_at: Utc::now().to_rfc3339(),
        })
    }

    pub fn claim(&self, id: &str) -> Result<ClaimDetail, ServiceError> {
        let id = id.trim();
        if id.is_empty() {
            return Err(ServiceError::NotFound("empty claim id".to_string()));
        }
        let resolved = self
            .store
            .resolve_claim_id(id)?
            .ok_or_else(|| ServiceError::NotFound(id.to_string()))?;
        self.store
            .claim_detail(&resolved)?
            .ok_or_else(|| ServiceError::NotFound(id.to_string()))
    }

    pub fn stats(&self) -> Result<LedgerStats, ServiceError> {
        self.store.ledger_stats().map_err(ServiceError::from)
    }

    pub fn settings(&self) -> Result<SettingsView, ServiceError> {
        let config = self.config.read().map_err(|_| ServiceError::Lock)?;
        Ok(settings::view(&config, &self.config_path))
    }

    pub fn save_settings(&self, update: SettingsUpdate) -> Result<SettingsView, ServiceError> {
        let mut next = {
            let current = self.config.read().map_err(|_| ServiceError::Lock)?;
            current.clone()
        };
        settings::apply_update(&mut next, update)?;
        next.save_to(&self.config_path)?;
        {
            let mut current = self.config.write().map_err(|_| ServiceError::Lock)?;
            *current = next;
        }
        self.settings()
    }

    /// Efektywna konfiguracja z env. Używana przez przyszły ingestion w UI.
    pub fn effective_config(&self) -> Result<Config, ServiceError> {
        let mut config = {
            let current = self.config.read().map_err(|_| ServiceError::Lock)?;
            current.clone()
        };
        config.apply_env();
        Ok(config)
    }

    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    pub async fn discover_models(&self) -> crate::discovery::DiscoveryResponse {
        crate::discovery::discover_local_engines().await
    }

    /// Bezpieczna analiza i dodanie źródła do lokalnego Evidence Ledgera.
    pub async fn ingest(&self, raw_url: &str) -> Result<IngestResponse, ServiceError> {
        let config = self.effective_config()?;
        let audit = AuditLog::open(expand_tilde(&config.security.audit_log))
            .map_err(|e| ServiceError::Internal(format!("cannot open audit log: {e}")))?;

        let fetcher =
            build_fetcher(&config, false).map_err(|e| ServiceError::Fetch(e.to_string()))?;
        let classifier =
            build_classifier(&config, false).map_err(|e| ServiceError::Pipeline(e.to_string()))?;

        let pipeline = EvidencePipeline::new(
            fetcher,
            classifier,
            Arc::clone(&self.store),
            audit,
            EvidenceOptions::default(),
        );

        let report = pipeline
            .ingest(raw_url, Lang::Auto)
            .await
            .map_err(|e| ServiceError::Pipeline(e.to_string()))?;

        let accepted: Vec<AcceptedClaimInfo> = report
            .accepted
            .into_iter()
            .map(|a| AcceptedClaimInfo {
                claim_id: a.claim_id,
                text: a.text,
                kind: a.kind.as_str().to_string(),
                stance: a.stance.as_str().to_string(),
                quote: a.quote,
                quote_hash: a.quote_hash,
                confidence: a.confidence,
            })
            .collect();

        let claims_found = accepted.len() + report.rejected.len();
        let claims_accepted = accepted.len();
        let claims_rejected = report.rejected.len();
        let title = if report.title.trim().is_empty() {
            "Dokument".to_string()
        } else {
            report.title
        };

        Ok(IngestResponse {
            url: report.canonical_url,
            title,
            domain: report.domain,
            claims_found,
            claims_accepted,
            claims_rejected,
            accepted,
            generated_at: Utc::now().to_rfc3339(),
        })
    }

    /// Asystent czatu BALD dla laików: obsługa URL, pytań o tematy i person-guarda.
    pub async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ServiceError> {
        let message = req.message.trim();
        if message.is_empty() {
            return Ok(ChatResponse {
                role: "assistant".to_string(),
                content:
                    "Wpisz pytanie, temat do zbadania lub wklej bezpośredni link (URL) do artykułu."
                        .to_string(),
                is_url: false,
                guard_refused: false,
                ingest: None,
                matching_claims: Vec::new(),
                sources: Vec::new(),
                auto_researched: false,
                new_claims_count: 0,
            });
        }

        // 1. Sprawdź czy wiadomość zawiera URL
        if let Some(url) = extract_url(message) {
            match self.ingest(&url).await {
                Ok(ingest_resp) => {
                    let text = format!(
                        "Zanalizowałem artykuł **{}** (`{}`).\n\n• Wyekstrahowano sprawdzalnych twierdzeń: **{}**\n• Potwierdzonych dosłownym cytatem (CitationGate): **{}**\n• Odrzuconych (brak pełnego cytatu): **{}**\n\nZweryfikowane twierdzenia zostały zapisane w Twoim lokalnym Rejestrze Twierdzeń.",
                        ingest_resp.title,
                        ingest_resp.domain,
                        ingest_resp.claims_found,
                        ingest_resp.claims_accepted,
                        ingest_resp.claims_rejected,
                    );
                    let count = ingest_resp.claims_accepted;
                    return Ok(ChatResponse {
                        role: "assistant".to_string(),
                        content: text,
                        is_url: true,
                        guard_refused: false,
                        ingest: Some(ingest_resp),
                        matching_claims: Vec::new(),
                        sources: Vec::new(),
                        auto_researched: false,
                        new_claims_count: count,
                    });
                }
                Err(err) => {
                    return Ok(ChatResponse {
                        role: "assistant".to_string(),
                        content: format!("Nie udało się pobrać lub zbadać podanego linku:\n`{err}`\n\nUpewnij się, że strona jest publicznie dostępna i nie narusza zasad bezpieczeństwa (SSRF)."),
                        is_url: true,
                        guard_refused: false,
                        ingest: None,
                        matching_claims: Vec::new(),
                        sources: Vec::new(),
                        auto_researched: false,
                        new_claims_count: 0,
                    });
                }
            }
        }

        // 2. Person Guard check - w czacie dopuszczamy długie teksty i fragmenty artykułów do analizy
        let verdict = RulesGate::new().check(&GateCtx {
            text: message,
            lang: Lang::Auto,
            purpose: GatePurpose::Action,
            citation_coverage: None,
            semantic_dispersion: None,
        });
        if matches!(verdict.decision, GateDecision::Refuse) {
            return Ok(ChatResponse {
                role: "assistant".to_string(),
                content: "Zgodnie z nadrzędną zasadą BALD (**„Temat tak, osoba nie”**), zapytania ukierunkowane na profilowanie osób prywatnych są blokowane. Chronimy prywatność i nie profilujemy osób.\n\nMożesz zapytać o zjawiska, organizacje, wydarzenia, twierdzenia, autorów książek i postacie publiczne — z chęcią je zbadam.".to_string(),
                is_url: false,
                guard_refused: true,
                ingest: None,
                matching_claims: Vec::new(),
                sources: Vec::new(),
                auto_researched: false,
                new_claims_count: 0,
            });
        }

        // 3. Live Web Research (Perplexity style):
        // Jeśli to zapytanie/temat (do 16 słów), zbadaj sieć na żywo za pomocą aktywnych dostawców (DuckDuckGo, Wikipedia, Searxng)
        let word_count = message.split_whitespace().count();
        let is_search_topic = word_count <= 16;

        let mut live_sources: Vec<ChatSource> = Vec::new();
        let mut new_claims_added = 0;

        if is_search_topic {
            if let Ok(search_resp) = self.search_web(message, 6).await {
                for hit in search_resp.hits {
                    if (hit.url.starts_with("http://") || hit.url.starts_with("https://"))
                        && !hit.url.contains("example.com")
                    {
                        live_sources.push(ChatSource {
                            title: hit.title,
                            url: hit.url,
                            domain: hit.domain,
                            snippet: hit.snippet,
                        });
                    }
                }
            }

            // Auto-ingest top 2 web sources do Rejestru Twierdzeń (z limitem czasu 3s na źródło)
            for src in live_sources.iter().take(2) {
                if let Ok(Ok(ing)) =
                    tokio::time::timeout(std::time::Duration::from_secs(3), self.ingest(&src.url))
                        .await
                {
                    new_claims_added += ing.claims_accepted;
                }
            }
        }

        // 5. Pobierz pasujące twierdzenia ze zaktualizowanego Store
        let all_claims = self.store.list_claims(200).unwrap_or_default();
        let query_words: Vec<&str> = message.split_whitespace().filter(|w| w.len() > 3).collect();
        let matching: Vec<ClaimSummary> = all_claims
            .into_iter()
            .filter(|c| {
                let lower = c.text.to_lowercase();
                query_words
                    .iter()
                    .any(|&w| lower.contains(&w.to_lowercase()))
            })
            .take(6)
            .collect();

        // 6. Jeśli skonfigurowany jest LLM, wygeneruj odpowiedź kontekstową z cytatami źródeł
        let effective_config = self.effective_config()?;
        if let Ok(client) = build_llm_client(&effective_config) {
            let mut context_block = String::new();

            if !live_sources.is_empty() {
                context_block
                    .push_str("\n\nAKTUALNE ŹRÓDŁA Z INTERNETU (cytuj jako [1], [2] itp.):\n");
                for (i, s) in live_sources.iter().enumerate() {
                    context_block.push_str(&format!(
                        "[{}] \"{}\" ({}) - {}\nURL: {}\n\n",
                        i + 1,
                        s.title,
                        s.domain,
                        s.snippet,
                        s.url
                    ));
                }
            }

            if !matching.is_empty() {
                context_block.push_str("\n\nZWERYFIKOWANE TWIERDZENIA W REJESTRZE BALD:\n");
                for c in matching.iter() {
                    context_block.push_str(&format!(
                        "- [{}] \"{}\" (liczba źródeł: {}, cytatów: {})\n",
                        c.kind.as_str(),
                        c.text,
                        c.document_count,
                        c.evidence_count
                    ));
                }
            }

            let system = "Jesteś BALD — autonomicznym silnikiem badawczym OSINT i weryfikacji faktów (Claim, Narrative & Source Evidence Engine), działającym na poziomie Perplexity/Firecrawl.\n\
            ZASADY ANALIZY:\n\
            1. Każdy fakt i twierdzenie popieraj numerowanym odnośnikiem źródłowym [1], [2] itp. odnoszącym się do dostarczonych źródeł internetowych.\n\
            2. NIE CENZURUJ ani nie usuwaj nazwisk osób publicznych, autorów, polityków, badaczy, urzędników i sygnalistów (np. Klaus Schwab, Rob Brotherton, Edward Snowden, autorzy ustaw i raportów). Wymieniaj ich z nazwiska i roli. Zasada prywatności dotyczy tylko osób prywatnych i wrażliwych danych PII (PESEL, adresy domowe).\n\
            3. Oddzielaj twarde fakty i dowody źródłowe od narracji, domniemań i spekulacji.\n\
            4. Na końcu odpowiedzi umieść czytelną sekcję '### Źródła' z ponumerowaną listą linków URL.\n\
            5. Odpowiadaj wyczerpująco, profesjonalnie, neutralnie i w języku polskim.";

            let prompt = format!("Pytanie / temat użytkownika: {message}{context_block}");

            if let Ok(answer) = client.complete_text(system, &prompt).await {
                return Ok(ChatResponse {
                    role: "assistant".to_string(),
                    content: answer,
                    is_url: false,
                    guard_refused: false,
                    ingest: None,
                    matching_claims: matching,
                    sources: live_sources,
                    auto_researched: true,
                    new_claims_count: new_claims_added,
                });
            }
        }

        // 7. Fallback jeśli LLM offline lub brak odpowiedzi
        let mut content = String::new();
        if !live_sources.is_empty() {
            content.push_str(&format!(
                "Wyniki zbadania sieci na żywo dla zapytania **{}**:\n\n",
                message
            ));
            for (i, s) in live_sources.iter().enumerate() {
                content.push_str(&format!(
                    "[{}] **{}** (`{}`)\n{}\n🔗 {}\n\n",
                    i + 1,
                    s.title,
                    s.domain,
                    s.snippet,
                    s.url
                ));
            }
        }

        if !matching.is_empty() {
            content.push_str(&format!(
                "\nZweryfikowane twierdzenia w Rejestrze BALD ({}):\n\n{}",
                matching.len(),
                matching
                    .iter()
                    .map(|c| format!(
                        "• **{}** (rodzaj: `{}`)\n  *Źródeł: {}, cytatów: {}*",
                        c.text,
                        c.kind.as_str(),
                        c.document_count,
                        c.evidence_count
                    ))
                    .collect::<Vec<_>>()
                    .join("\n\n")
            ));
        }

        if content.is_empty() {
            content = "Nie znaleziono jeszcze wystarczających informacji na ten temat w sieci ani w lokalnej bazie. Możesz wkleić bezpośredni link (URL) do artykułu lub dokumentu, a zbadam go pod kątem twierdzeń i cytatów.".to_string();
        }

        Ok(ChatResponse {
            role: "assistant".to_string(),
            content,
            is_url: false,
            guard_refused: false,
            ingest: None,
            matching_claims: matching,
            sources: live_sources,
            auto_researched: true,
            new_claims_count: new_claims_added,
        })
    }

    /// Wyszukiwanie w internecie przez aktywne providery (SearXNG / Brave / Demo)
    /// z automatyczną ochroną Person Guard.
    pub async fn search_web(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<SearchWebResponse, ServiceError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(SearchWebResponse {
                query: String::new(),
                hits: Vec::new(),
                guard_refused: false,
                guard_message: None,
                total: 0,
                latency_ms: 0,
            });
        }

        // 1. Person Guard
        let verdict = RulesGate::new().check(&GateCtx::search(query, Lang::Auto));
        if matches!(verdict.decision, GateDecision::Refuse) {
            return Ok(SearchWebResponse {
                query: query.to_string(),
                hits: Vec::new(),
                guard_refused: true,
                guard_message: Some(
                    "Zgodnie z zasadą BALD („Temat tak, osoba nie”), wyszukiwanie osób prywatnych jest zablokowane. Szukaj tematów, instytucji, zjawisk i wydarzeń.".to_string(),
                ),
                total: 0,
                latency_ms: 0,
            });
        }

        let config = self.effective_config()?;
        let audit = AuditLog::open(expand_tilde(&config.security.audit_log))
            .map_err(|e| ServiceError::Internal(format!("cannot open audit log: {e}")))?;

        let providers = build_search_providers(&config);
        let run_store: Arc<dyn RunStore> = self.store.clone();
        let engine = SearchEngine::new(providers, audit, Some(run_store));

        let opts = SearchOptions {
            limit: limit.clamp(1, 50),
            store: true,
        };

        let report = engine
            .search(query, Lang::Auto, &opts)
            .await
            .map_err(|e| ServiceError::Pipeline(e.to_string()))?;

        let mut hits = report
            .hits
            .into_iter()
            .map(|h| {
                let domain = domain_of(&h.canonical_url).to_string();
                SearchHit {
                    title: h.title,
                    url: h.canonical_url,
                    domain,
                    snippet: h.snippet,
                    score: h.score,
                    providers: h.providers,
                }
            })
            .collect::<Vec<_>>();

        if hits.is_empty() {
            let demo = DemoProvider::new();
            if let Ok(demo_hits) = demo
                .search(&bse_core::SearchQuery::new(
                    query.to_string(),
                    Lang::Auto,
                    opts.limit,
                ))
                .await
            {
                for (idx, dh) in demo_hits.into_iter().enumerate() {
                    let domain = domain_of(&dh.url).to_string();
                    hits.push(SearchHit {
                        title: dh.title,
                        url: dh.url,
                        domain,
                        snippet: dh.snippet,
                        score: 1.0 / (idx + 1) as f32,
                        providers: vec!["demo".to_string()],
                    });
                }
            }
        }

        let total = hits.len();
        Ok(SearchWebResponse {
            query: query.to_string(),
            hits,
            guard_refused: false,
            guard_message: None,
            total,
            latency_ms: report.latency_ms,
        })
    }

    pub fn list_bots(&self) -> Result<Vec<ResearchBot>, ServiceError> {
        Ok(self.store.list_bots()?)
    }

    pub fn create_bot(&self, req: NewResearchBot) -> Result<ResearchBot, ServiceError> {
        if req.name.trim().is_empty() {
            return Err(ServiceError::Pipeline(
                "Nazwa bota nie może być pusta".to_string(),
            ));
        }
        if req.query.trim().is_empty() {
            return Err(ServiceError::Pipeline(
                "Fraza wyszukiwania nie może być pusta".to_string(),
            ));
        }
        Ok(self.store.create_bot(&req)?)
    }

    pub fn get_bot(&self, id: &str) -> Result<ResearchBot, ServiceError> {
        self.store
            .get_bot(id)?
            .ok_or_else(|| ServiceError::NotFound(format!("Bot {id} nie został znaleziony")))
    }

    pub fn update_bot(&self, bot: ResearchBot) -> Result<(), ServiceError> {
        Ok(self.store.update_bot(&bot)?)
    }

    pub fn delete_bot(&self, id: &str) -> Result<bool, ServiceError> {
        Ok(self.store.delete_bot(id)?)
    }

    pub fn list_bot_runs(
        &self,
        bot_id: &str,
        limit: usize,
    ) -> Result<Vec<BotRunRecord>, ServiceError> {
        Ok(self.store.list_bot_runs(bot_id, limit.clamp(1, 100))?)
    }

    pub fn due_bots(&self) -> Result<Vec<ResearchBot>, ServiceError> {
        Ok(self.store.due_bots()?)
    }

    pub async fn execute_bot(&self, bot_id: &str) -> Result<BotRunRecord, ServiceError> {
        let mut bot = self
            .store
            .get_bot(bot_id)?
            .ok_or_else(|| ServiceError::NotFound(format!("Bot {bot_id} nie został znaleziony")))?;

        let started_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let mut urls_checked = 0;
        let mut claims_added = 0;
        let mut error_msg = None;
        let status;

        match self.search_web(&bot.query, bot.max_sources_per_run).await {
            Ok(search_resp) => {
                if search_resp.guard_refused {
                    error_msg = search_resp.guard_message;
                    status = "refused".to_string();
                } else {
                    for hit in search_resp.hits {
                        // Skip if already in database
                        if let Ok(true) = self.store.has_document(&hit.url) {
                            continue;
                        }
                        urls_checked += 1;
                        if bot.auto_ingest {
                            match self.ingest(&hit.url).await {
                                Ok(ingest_resp) => {
                                    claims_added += ingest_resp.claims_accepted;
                                }
                                Err(e) => {
                                    if error_msg.is_none() {
                                        error_msg =
                                            Some(format!("Błąd badania {}: {}", hit.url, e));
                                    }
                                }
                            }
                        }
                    }
                    status = if error_msg.is_some() && claims_added == 0 && urls_checked > 0 {
                        "partial".to_string()
                    } else {
                        "success".to_string()
                    };
                }
            }
            Err(e) => {
                error_msg = Some(e.to_string());
                status = "failed".to_string();
            }
        }

        let finished_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let run_record = BotRunRecord {
            id: Uuid::new_v4().to_string(),
            bot_id: bot.id.clone(),
            started_at,
            finished_at,
            urls_checked,
            claims_added,
            error_msg,
            status,
        };

        self.store.record_bot_run(&run_record)?;

        bot.run_count += 1;
        bot.claims_found += claims_added as u64;
        bot.last_run_at = Some(run_record.started_at.clone());
        let next_run = Utc::now() + chrono::Duration::minutes(bot.interval_mins as i64);
        bot.next_run_at = Some(next_run.to_rfc3339_opts(SecondsFormat::Millis, true));
        self.store.update_bot(&bot)?;

        Ok(run_record)
    }
}

fn build_search_providers(cfg: &Config) -> Vec<Arc<dyn Provider>> {
    let mut providers: Vec<Arc<dyn Provider>> = Vec::new();

    // 1. Live Web Search (DuckDuckGo HTML crawler) - bezkluczowy, wolny od cenzury silnik przeszukiwania otwartej sieci
    if let Ok(ddg) = DuckDuckGoProvider::new() {
        providers.push(Arc::new(ddg));
    }

    if cfg.providers.searxng.enabled {
        if let Ok(provider) = SearxngProvider::new(
            cfg.searxng_base_url(),
            cfg.providers.searxng.language.clone(),
            cfg.providers.searxng.safesearch,
        ) {
            providers.push(Arc::new(provider));
        }
    }

    if cfg.providers.brave.enabled {
        if let Ok(provider) = BraveProvider::new(
            cfg.providers.brave.api_key.clone(),
            cfg.providers.brave.country.clone(),
        ) {
            providers.push(Arc::new(provider));
        }
    }

    // Dodaj otwarte źródła wiedzy Wikipedia (PL & EN) - bezkluczowe, publiczne źródła internetowe
    if let Ok(wiki_pl) = WikipediaProvider::new("pl") {
        providers.push(Arc::new(wiki_pl));
    }
    if let Ok(wiki_en) = WikipediaProvider::new("en") {
        providers.push(Arc::new(wiki_en));
    }

    if providers.is_empty() {
        providers.push(Arc::new(DemoProvider::new()));
    }

    providers
}

pub fn extract_url(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        let trimmed = word.trim_matches(|c| {
            c == '<'
                || c == '>'
                || c == '('
                || c == ')'
                || c == '['
                || c == ']'
                || c == '"'
                || c == '\''
                || c == ','
                || c == ';'
                || c == '.'
        });
        if (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
            && url::Url::parse(trimmed).is_ok()
        {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn classify_options(cfg: &Config) -> ClassifyOptions {
    ClassifyOptions {
        max_input_chars: cfg.llm.max_input_chars.max(1_000),
        max_claims: cfg.llm.max_claims.clamp(1, 50),
        max_attempts: cfg.llm.max_attempts.clamp(1, 4),
        min_confidence: 0.0,
    }
}

fn build_llm_client(cfg: &Config) -> Result<LlmClient, anyhow::Error> {
    let provider = cfg.llm.provider.trim().to_ascii_lowercase();
    let llm_prov = LlmProvider::parse(&provider)
        .ok_or_else(|| anyhow::anyhow!("unknown LLM provider: {provider}"))?;
    let settings = LlmSettings {
        provider: llm_prov,
        base_url: cfg.llm.base_url.clone(),
        model: cfg.llm.model.clone(),
        api_key: cfg.llm.api_key(),
        timeout: Duration::from_secs(cfg.llm.timeout_secs.max(1)),
        connect_timeout: Duration::from_secs(cfg.llm.connect_timeout_secs.max(1)),
        temperature: cfg.llm.temperature,
        max_tokens: cfg.llm.max_tokens.max(64),
    };
    LlmClient::new(settings).map_err(|e| anyhow::anyhow!(e.to_string()))
}

fn classifier_backend(
    cfg: &Config,
    provider: &str,
    base_url: &str,
    model: &str,
    api_key: Option<String>,
) -> Result<Arc<dyn Classifier>, anyhow::Error> {
    let provider = provider.trim().to_ascii_lowercase();
    match provider.as_str() {
        "none" | "heuristic" => Ok(Arc::new(HeuristicClassifier::new(classify_options(cfg)))),
        "ollama" | "openai" => {
            let provider = LlmProvider::parse(&provider)
                .ok_or_else(|| anyhow::anyhow!("unknown LLM provider: {provider}"))?;
            let settings = LlmSettings {
                provider,
                base_url: base_url.to_string(),
                model: model.to_string(),
                api_key,
                timeout: Duration::from_secs(cfg.llm.timeout_secs.max(1)),
                connect_timeout: Duration::from_secs(cfg.llm.connect_timeout_secs.max(1)),
                temperature: cfg.llm.temperature,
                max_tokens: cfg.llm.max_tokens.max(64),
            };
            let client = LlmClient::new(settings).map_err(|e| anyhow::anyhow!(e.to_string()))?;
            Ok(Arc::new(LlmClassifier::new(
                Arc::new(client),
                classify_options(cfg),
            )))
        }
        other => Err(anyhow::anyhow!("unknown LLM provider: {other}")),
    }
}

fn build_classifier(cfg: &Config, offline: bool) -> Result<Arc<dyn Classifier>, anyhow::Error> {
    if offline {
        return Ok(Arc::new(HeuristicClassifier::new(classify_options(cfg))));
    }

    let primary = classifier_backend(
        cfg,
        &cfg.llm.provider,
        &cfg.llm.base_url,
        &cfg.llm.model,
        cfg.llm.api_key(),
    )?;
    if matches!(
        cfg.llm.provider.trim().to_ascii_lowercase().as_str(),
        "none" | "heuristic"
    ) || cfg
        .llm
        .fallback_provider
        .trim()
        .eq_ignore_ascii_case("none")
        || cfg
            .llm
            .fallback_provider
            .trim()
            .eq_ignore_ascii_case(&cfg.llm.provider)
    {
        return Ok(primary);
    }

    let fallback = classifier_backend(
        cfg,
        &cfg.llm.fallback_provider,
        &cfg.llm.fallback_base_url,
        &cfg.llm.fallback_model,
        cfg.llm.fallback_api_key(),
    )?;
    Ok(Arc::new(FallbackClassifier::new(primary, fallback)))
}

fn build_fetcher(cfg: &Config, no_cache: bool) -> Result<Fetcher, anyhow::Error> {
    let options = FetchOptions {
        max_bytes: cfg.fetch.max_bytes.max(1),
        max_redirects: cfg.fetch.max_redirects,
        timeout: Duration::from_secs(cfg.fetch.timeout_secs.max(1)),
        respect_robots: cfg.fetch.respect_robots,
        ..FetchOptions::default()
    };
    let fetcher = Fetcher::new(options).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    if no_cache {
        return Ok(fetcher);
    }

    let cache = FetchCache::new(
        expand_tilde(&cfg.fetch.cache_dir),
        Duration::from_secs(cfg.fetch.cache_ttl_secs),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(fetcher.with_cache(cache))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config() -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "bald-api-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let config_path = dir.join("config.toml");
        let db_path = dir.join("bald.db");
        let mut config = Config::default();
        config.storage.path = db_path.to_string_lossy().to_string();
        config.llm.provider = "disabled".to_string();
        config.save_to(&config_path).expect("save config");
        (dir, config_path)
    }

    #[test]
    fn empty_ledger_and_settings_roundtrip() {
        let (dir, path) = temp_config();
        let service = Service::open_at(path.clone()).expect("service");
        let claims = service.claims(20).expect("claims");
        assert!(claims.claims.is_empty());
        assert_eq!(claims.stats.claims, 0);

        let update = SettingsUpdate {
            searxng_enabled: Some(true),
            searxng_base_url: Some("http://127.0.0.1:8080".to_string()),
            llm_model: Some("qwen3:8b".to_string()),
            ..SettingsUpdate::default()
        };
        let view = service.save_settings(update).expect("save settings");
        assert!(view.searxng.enabled);
        assert_eq!(view.llm.model, "qwen3:8b");

        let reloaded = Config::load_from(&path).expect("reload");
        assert!(reloaded.providers.searxng.enabled);
        assert_eq!(reloaded.llm.model, "qwen3:8b");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn claim_lookup_rejects_unknown_id() {
        let (dir, path) = temp_config();
        let service = Service::open_at(path).expect("service");
        assert!(matches!(
            service.claim("sha256:missing"),
            Err(ServiceError::NotFound(_))
        ));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn extract_url_works() {
        assert_eq!(
            extract_url("zobacz: https://example.com/artykul."),
            Some("https://example.com/artykul".to_string())
        );
        assert_eq!(
            extract_url("<https://test.pl/news?id=123>"),
            Some("https://test.pl/news?id=123".to_string())
        );
        assert_eq!(extract_url("zwykły tekst bez linku"), None);
    }

    #[tokio::test]
    async fn chat_blocks_person_targeted_query() {
        let (dir, path) = temp_config();
        let service = Service::open_at(path).expect("service");
        let resp = service
            .chat(ChatRequest {
                message: "gdzie mieszka jan kowalski adres pesel".to_string(),
                history: None,
            })
            .await
            .expect("chat");
        assert!(resp.guard_refused);
        assert!(resp.content.contains("Temat tak, osoba nie"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn chat_allows_long_text_and_author_names() {
        let (dir, path) = temp_config();
        let service = Service::open_at(path).expect("service");
        let long_text = "Kontekst Teorie spiskowe nie sa nowym zjawiskiem. Historia ludzkosci jest pelna teorii i narracji spiskowych opartych na domyslach i niepotwierdzonych dowodach. Jako powiedzial Rob Brotherton nie kazda teoria na temat spisku moze byc uznana za teorie spiskowa.";
        let resp = service
            .chat(ChatRequest {
                message: long_text.to_string(),
                history: None,
            })
            .await
            .expect("chat");
        assert!(!resp.guard_refused);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn search_web_refuses_person_query() {
        let (dir, path) = temp_config();
        let service = Service::open_at(path).expect("service");
        let resp = service
            .search_web("gdzie mieszka jan kowalski numer telefonu", 10)
            .await
            .expect("search");
        assert!(resp.guard_refused);
        assert!(resp.hits.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn search_web_returns_results_for_benign_query() {
        let (dir, path) = temp_config();
        let service = Service::open_at(path).expect("service");
        let resp = service
            .search_web("Great Reset Klaus Schwab WEF", 5)
            .await
            .expect("search");
        assert!(!resp.guard_refused);
        // Returns results from Demo fixtures if SearXNG/Brave are not running
        assert!(!resp.hits.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn bot_researcher_lifecycle_and_execution() {
        let (dir, path) = temp_config();
        let service = Service::open_at(path).expect("service");

        let bot = service
            .create_bot(NewResearchBot {
                name: "Test Bot CBDC".to_string(),
                query: "cyfrowe euro".to_string(),
                interval_mins: 120,
                max_sources_per_run: Some(2),
                auto_ingest: Some(false),
            })
            .expect("create bot");

        assert_eq!(bot.name, "Test Bot CBDC");
        assert_eq!(bot.interval_mins, 120);

        let bots = service.list_bots().expect("list bots");
        assert_eq!(bots.len(), 1);

        let run = service.execute_bot(&bot.id).await.expect("execute bot");
        assert_eq!(run.bot_id, bot.id);
        assert_eq!(run.status, "success");

        let runs = service.list_bot_runs(&bot.id, 10).expect("list runs");
        assert_eq!(runs.len(), 1);

        let updated_bot = service.get_bot(&bot.id).expect("get bot");
        assert_eq!(updated_bot.run_count, 1);
        assert!(updated_bot.last_run_at.is_some());

        assert!(service.delete_bot(&bot.id).expect("delete bot"));
        assert!(service.list_bots().expect("list bots").is_empty());

        let _ = std::fs::remove_dir_all(dir);
    }
}
