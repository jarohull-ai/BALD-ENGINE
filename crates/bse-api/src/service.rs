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
use bse_llm::{LlmClient, LlmProvider, LlmSettings};
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
pub struct EmbeddedModelStatusItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
    pub installed: bool,
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddedModelsResponse {
    pub active_provider: String,
    pub current_model_id: String,
    pub models: Vec<EmbeddedModelStatusItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSource {
    pub title: String,
    pub url: String,
    pub domain: String,
    pub snippet: String,
    #[serde(default)]
    pub published: Option<String>,
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
    #[serde(default)]
    pub follow_ups: Vec<String>,
    #[serde(default)]
    pub credibility: Option<crate::credibility::AnswerCredibility>,
    #[serde(default)]
    pub in_trial: bool,
    #[serde(default)]
    pub trial_model: Option<String>,
    #[serde(default)]
    pub previous_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub domain: String,
    pub snippet: String,
    pub score: f32,
    pub providers: Vec<String>,
    #[serde(default)]
    pub published: Option<String>,
    #[serde(default)]
    pub credibility: Option<crate::credibility::CredibilityScore>,
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

    pub fn list_embedded_models(&self) -> Result<EmbeddedModelsResponse, ServiceError> {
        let config = self.effective_config()?;
        let storage = bse_llm::ModelStorage::default();
        let current_id = config.llm.embedded_model.clone();
        let models = bse_llm::ModelCatalog::available_models()
            .into_iter()
            .map(|preset| {
                let installed = storage.is_installed(&preset);
                let is_current = preset.id == current_id;
                EmbeddedModelStatusItem {
                    id: preset.id,
                    name: preset.name,
                    description: preset.description,
                    size_bytes: preset.size_bytes,
                    installed,
                    is_current,
                }
            })
            .collect();

        Ok(EmbeddedModelsResponse {
            active_provider: config.llm.provider.clone(),
            current_model_id: current_id,
            models,
        })
    }

    pub async fn download_embedded_model(&self, model_id: &str) -> Result<bool, ServiceError> {
        let preset = bse_llm::ModelCatalog::find(model_id)
            .ok_or_else(|| ServiceError::NotFound(format!("Model {model_id} nie został znaleziony")))?;
        let storage = bse_llm::ModelStorage::default();
        if storage.is_installed(&preset) {
            return Ok(true);
        }
        storage
            .download(&preset, |_downloaded, _total| {})
            .await
            .map_err(|e| ServiceError::Internal(format!("Błąd pobierania modelu: {e}")))?;
        Ok(true)
    }

    pub async fn start_trial(
        &self,
        model_id: &str,
    ) -> Result<crate::scout::TrialActionResponse, ServiceError> {
        let model_id = model_id.trim();
        let preset = bse_llm::ModelCatalog::find(model_id).ok_or_else(|| {
            ServiceError::NotFound(format!("Nie znaleziono modelu {model_id}"))
        })?;

        let storage = bse_llm::ModelStorage::default();
        if !storage.is_installed(&preset) {
            storage
                .download(&preset, |_d, _t| {})
                .await
                .map_err(|e| ServiceError::Internal(format!("Błąd pobierania modelu: {e}")))?;
        }

        let mut next = {
            let current = self.config.read().map_err(|_| ServiceError::Lock)?;
            current.clone()
        };

        let previous_model = if next.scout.in_trial {
            next.scout.previous_model.clone()
        } else {
            Some(next.llm.embedded_model.clone())
        };

        next.scout.in_trial = true;
        next.scout.trial_model = Some(model_id.to_string());
        next.scout.previous_model = previous_model;
        next.llm.provider = "embedded".to_string();
        next.llm.embedded_model = model_id.to_string();

        next.save_to(&self.config_path)?;
        {
            let mut current = self.config.write().map_err(|_| ServiceError::Lock)?;
            *current = next;
        }

        Ok(crate::scout::TrialActionResponse {
            success: true,
            in_trial: true,
            current_model: model_id.to_string(),
            message: format!(
                "Rozpoczęto jazdę próbną modelu {}. Przetestuj jego odpowiedzi w czacie!",
                preset.name
            ),
        })
    }

    pub fn commit_trial(&self) -> Result<crate::scout::TrialActionResponse, ServiceError> {
        let mut next = {
            let current = self.config.read().map_err(|_| ServiceError::Lock)?;
            current.clone()
        };

        if !next.scout.in_trial {
            return Ok(crate::scout::TrialActionResponse {
                success: false,
                in_trial: false,
                current_model: next.llm.embedded_model.clone(),
                message: "Brak aktywnej jazdy próbnej.".to_string(),
            });
        }

        let chosen = next
            .scout
            .trial_model
            .clone()
            .unwrap_or_else(|| next.llm.embedded_model.clone());

        next.llm.provider = "embedded".to_string();
        next.llm.embedded_model = chosen.clone();
        next.scout.in_trial = false;
        next.scout.trial_model = None;
        next.scout.previous_model = None;

        next.save_to(&self.config_path)?;
        {
            let mut current = self.config.write().map_err(|_| ServiceError::Lock)?;
            *current = next;
        }

        Ok(crate::scout::TrialActionResponse {
            success: true,
            in_trial: false,
            current_model: chosen.clone(),
            message: format!("Model {chosen} został pomyślnie zapisany jako Twój domyślny model AI!"),
        })
    }

    pub fn rollback_trial(&self) -> Result<crate::scout::TrialActionResponse, ServiceError> {
        let mut next = {
            let current = self.config.read().map_err(|_| ServiceError::Lock)?;
            current.clone()
        };

        if !next.scout.in_trial {
            return Ok(crate::scout::TrialActionResponse {
                success: false,
                in_trial: false,
                current_model: next.llm.embedded_model.clone(),
                message: "Brak aktywnej jazdy próbnej do wycofania.".to_string(),
            });
        }

        let fallback = next
            .scout
            .previous_model
            .clone()
            .unwrap_or_else(|| "qwen2.5-0.5b-instruct".to_string());

        next.llm.embedded_model = fallback.clone();
        next.scout.in_trial = false;
        next.scout.trial_model = None;
        next.scout.previous_model = None;

        next.save_to(&self.config_path)?;
        {
            let mut current = self.config.write().map_err(|_| ServiceError::Lock)?;
            *current = next;
        }

        Ok(crate::scout::TrialActionResponse {
            success: true,
            in_trial: false,
            current_model: fallback.clone(),
            message: format!("Pomyślnie przywrócono poprzedni model ({fallback})."),
        })
    }

    pub async fn scout_report(&self) -> Result<crate::scout::ScoutReport, ServiceError> {
        let config = self.effective_config()?;
        Ok(crate::scout::get_scout_report(&config).await)
    }

    pub async fn scout_check(&self) -> Result<crate::scout::ScoutReport, ServiceError> {
        let mut next = {
            let current = self.config.read().map_err(|_| ServiceError::Lock)?;
            current.clone()
        };
        next.scout.last_run = Some(chrono::Utc::now().format("%Y-%m-%d %H:%M").to_string());
        let _ = next.save_to(&self.config_path);
        {
            let mut current = self.config.write().map_err(|_| ServiceError::Lock)?;
            *current = next.clone();
        }
        Ok(crate::scout::get_scout_report(&next).await)
    }

    /// Bezpieczna analiza i dodanie źródła do lokalnego Evidence Ledgera.
    pub async fn ingest(&self, raw_url: &str) -> Result<IngestResponse, ServiceError> {
        self.ingest_internal(raw_url, false).await
    }

    /// Wewnętrzna wersja ingest z możliwością wymuszenia szybkiego HeuristicClassifier (np. dla auto-badania w czacie).
    pub async fn ingest_internal(
        &self,
        raw_url: &str,
        force_heuristic: bool,
    ) -> Result<IngestResponse, ServiceError> {
        let config = self.effective_config()?;
        let audit = AuditLog::open(expand_tilde(&config.security.audit_log))
            .map_err(|e| ServiceError::Internal(format!("cannot open audit log: {e}")))?;

        let fetcher =
            build_fetcher(&config, false).map_err(|e| ServiceError::Fetch(e.to_string()))?;
        let classifier: Arc<dyn Classifier> = if force_heuristic {
            Arc::new(bse_classify::HeuristicClassifier::default())
        } else {
            build_classifier(&config, false).map_err(|e| ServiceError::Pipeline(e.to_string()))?
        };

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
        let effective_config = self.effective_config().unwrap_or_default();
        let in_trial = effective_config.scout.in_trial;
        let trial_model = effective_config.scout.trial_model.clone();
        let previous_model = effective_config.scout.previous_model.clone();

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
                follow_ups: Vec::new(),
                credibility: None,
                in_trial,
                trial_model,
                previous_model,
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
                    let credibility = Some(crate::credibility::evaluate_answer(
                        &text,
                        &[ChatSource {
                            title: ingest_resp.title.clone(),
                            url: ingest_resp.url.clone(),
                            domain: ingest_resp.domain.clone(),
                            snippet: text.clone(),
                            published: None,
                        }],
                        &[],
                    ));
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
                        follow_ups: Vec::new(),
                        credibility,
                        in_trial,
                        trial_model,
                        previous_model,
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
                        follow_ups: Vec::new(),
                        credibility: None,
                        in_trial,
                        trial_model,
                        previous_model,
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
                follow_ups: Vec::new(),
                credibility: None,
                in_trial,
                trial_model,
                previous_model,
            });
        }

        // 3. Live Web Research (Perplexity style):
        // Jeśli to zapytanie/temat (do 16 słów), zbadaj sieć na żywo za pomocą aktywnych dostawców (DuckDuckGo, Wikipedia, Searxng)
        let word_count = message.split_whitespace().count();
        let is_search_topic = word_count <= 16;

        let mut live_sources: Vec<ChatSource> = Vec::new();
        let mut new_claims_added = 0;

        if is_search_topic {
            if let Ok(search_resp) = self.search_web(message, 8).await {
                for hit in search_resp.hits {
                    if (hit.url.starts_with("http://") || hit.url.starts_with("https://"))
                        && !hit.url.contains("example.com")
                    {
                        live_sources.push(ChatSource {
                            title: hit.title,
                            url: hit.url,
                            domain: hit.domain,
                            snippet: hit.snippet,
                            published: hit.published,
                        });
                    }
                }
            }

            // Posortuj źródła chronologicznie: od najnowszych do najstarszych
            live_sources.sort_by(|a, b| match (&a.published, &b.published) {
                (Some(da), Some(db)) => db.cmp(da),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            });

            // Auto-ingest top 2 web sources do Rejestru Twierdzeń (błyskawiczny HeuristicClassifier z CitationGate, bez blokowania modelu LLM)
            for src in live_sources.iter().take(2) {
                if let Ok(ing) = self.ingest_internal(&src.url, true).await {
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
        let is_embedded = effective_config.llm.provider == "embedded";
        if let Ok(client) = build_llm_client(&effective_config) {
            let mut context_block = String::new();

            let max_src_count = if is_embedded { 3 } else { 8 };
            let max_claims_count = if is_embedded { 2 } else { 6 };
            let snippet_max_chars = if is_embedded { 180 } else { 400 };

            if !live_sources.is_empty() {
                context_block.push_str(
                    "\n\nAKTUALNE ŹRÓDŁA Z INTERNETU (cytuj jako [1], [2] itp.):\n",
                );
                for (i, s) in live_sources.iter().take(max_src_count).enumerate() {
                    let date_str = s
                        .published
                        .as_deref()
                        .map(|d| format!(" [Data: {d}]"))
                        .unwrap_or_default();
                    let snip = if s.snippet.len() > snippet_max_chars {
                        let mut end = snippet_max_chars;
                        while !s.snippet.is_char_boundary(end) && end > 0 {
                            end -= 1;
                        }
                        &s.snippet[..end]
                    } else {
                        &s.snippet
                    };
                    context_block.push_str(&format!(
                        "[{}] \"{}\" ({}){} - {}\nURL: {}\n\n",
                        i + 1,
                        s.title,
                        s.domain,
                        date_str,
                        snip,
                        s.url
                    ));
                }
            }

            if !matching.is_empty() {
                context_block.push_str("\n\nZWERYFIKOWANE TWIERDZENIA W REJESTRZE BALD:\n");
                for c in matching.iter().take(max_claims_count) {
                    context_block.push_str(&format!(
                        "- [{}] \"{}\" (liczba źródeł: {}, cytatów: {})\n",
                        c.kind.as_str(),
                        c.text,
                        c.document_count,
                        c.evidence_count
                    ));
                }
            }

            let system = if is_embedded {
                "Jesteś BALD — zwięzłym, polskim silnikiem badawczym OSINT i faktów. Odpowiadaj konkretnie na pytanie, popieraj fakty odnośnikami [1], [2] ze źródeł i podsumuj w 2-3 akapitach po polsku. Na końcu wymień sekcję ### Źródła oraz 2-3 ### Proponowane kierunki śledztwa."
            } else {
                "Jesteś BALD — autonomicznym silnikiem badawczym OSINT i weryfikacji faktów (Claim, Narrative & Source Evidence Engine), działającym na poziomie Perplexity/Firecrawl.\n\
                ZASADY ANALIZY:\n\
                1. Każdy fakt i twierdzenie popieraj numerowanym odnośnikiem źródłowym [1], [2] itp. odnoszącym się do dostarczonych źródeł internetowych.\n\
                2. CHRONOLOGIA: Informacje i wydarzenia w tekście oraz listę źródeł przedstawiaj w porządku chronologicznym — od najnowszych publikacji i doniesień do najstarszych źródeł i korzeni historycznych. Przy kluczowych faktach podawaj daty lub lata publikacji i zdarzeń.\n\
                3. NIE CENZURUJ ani nie usuwaj nazwisk osób publicznych, autorów, polityków, badaczy, urzędników i sygnalistów (np. Klaus Schwab, Rob Brotherton, Edward Snowden, autorzy ustaw i raportów). Wymieniaj ich z nazwiska i roli. Zasada prywatności dotyczy tylko osób prywatnych i wrażliwych danych PII (PESEL, adresy domowe).\n\
                4. Oddzielaj twarde fakty i dowody źródłowe od narracji, domniemań i spekulacji.\n\
                5. Na końcu odpowiedzi ZAWSZE umieść:\n\
                   - Sekcję '### Źródła' z ponumerowaną listą linków URL uporządkowaną chronologicznie,\n\
                   - Sekcję '### Proponowane kierunki śledztwa:' zawierającą dokładnie 3 intrygujące, konkretne pytania lub wątki pogłębiające sprawę (np. konkretne podziemia, artefakty, klątwy czy dokumenty).\n\
                6. Odpowiadaj wyczerpująco, profesjonalnie, neutralnie i w języku polskim."
            };

            let prompt = format!("Pytanie / temat użytkownika: {message}{context_block}");

            if let Ok(answer) = client.complete_text(system, &prompt).await {
                let follow_ups = extract_follow_ups(&answer, message);
                let credibility = Some(crate::credibility::evaluate_answer(
                    &answer,
                    &live_sources,
                    &matching,
                ));
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
                    follow_ups,
                    credibility,
                    in_trial,
                    trial_model,
                    previous_model,
                });
            }
        }

        // 7. Fallback jeśli LLM offline lub brak odpowiedzi
        let mut content = String::new();
        if !live_sources.is_empty() {
            content.push_str(&format!(
                "Wyniki zbadania sieci na żywo dla zapytania **{}** (uporządkowane chronologicznie):\n\n",
                message
            ));
            for (i, s) in live_sources.iter().enumerate() {
                let date_str = s
                    .published
                    .as_deref()
                    .map(|d| format!(" ({d})"))
                    .unwrap_or_default();
                content.push_str(&format!(
                    "[{}] **{}** (`{}`){}\n{}\n🔗 {}\n\n",
                    i + 1,
                    s.title,
                    s.domain,
                    date_str,
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

        let follow_ups = extract_follow_ups(&content, message);
        let credibility = Some(crate::credibility::evaluate_answer(
            &content,
            &live_sources,
            &matching,
        ));

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
            follow_ups,
            credibility,
            in_trial,
            trial_model,
            previous_model,
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
                let published = h
                    .published
                    .or_else(|| extract_date_heuristic(&h.snippet, &h.canonical_url));
                SearchHit {
                    title: h.title,
                    url: h.canonical_url,
                    domain,
                    snippet: h.snippet,
                    score: h.score,
                    providers: h.providers,
                    published,
                    credibility: None,
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
                        published: None,
                        credibility: None,
                    });
                }
            }
        }

        // Posortuj trafienia chronologicznie: od najnowszych do najstarszych
        hits.sort_by(|a, b| match (&a.published, &b.published) {
            (Some(da), Some(db)) => db.cmp(da),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });

        // Oblicz transparentny wskaźnik wiarygodności dla każdego źródła (Evidence & Credibility Judge)
        let matching_claims = self.store.list_claims(50).unwrap_or_default();
        let all_hits_clone = hits.clone();
        for hit in &mut hits {
            hit.credibility = Some(crate::credibility::evaluate_hit(
                hit,
                &all_hits_clone,
                &matching_claims,
            ));
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

pub fn extract_date_heuristic(snippet: &str, url: &str) -> Option<String> {
    use regex::Regex;
    use std::sync::OnceLock;

    static RELATIVE_RE: OnceLock<Regex> = OnceLock::new();
    let rel_re = RELATIVE_RE.get_or_init(|| {
        Regex::new(
            r#"(?i)\b(\d+)\s*(godz|godzin|dni|dn|hours?|days?|minut|minutes?)\s*(temu|ago)\b"#,
        )
        .unwrap()
    });

    if rel_re.is_match(snippet) || snippet.contains("wczoraj") || snippet.contains("yesterday") {
        return Some(chrono::Utc::now().format("%Y-%m-%d").to_string());
    }

    // 1. ISO date: YYYY-MM-DD
    static ISO_RE: OnceLock<Regex> = OnceLock::new();
    let iso_re = ISO_RE.get_or_init(|| {
        Regex::new(r#"\b(20[12]\d)-(0[1-9]|1[0-2])-(0[1-9]|[12]\d|3[01])\b"#).unwrap()
    });
    if let Some(cap) = iso_re.captures(snippet).or_else(|| iso_re.captures(url)) {
        return Some(cap[0].to_string());
    }

    // 2. URL date: /YYYY/MM/DD or /YYYY/MM/
    static URL_DATE_RE: OnceLock<Regex> = OnceLock::new();
    let url_re = URL_DATE_RE.get_or_init(|| {
        Regex::new(r#"/(20[12]\d)/(0[1-9]|1[0-2])(?:/(0[1-9]|[12]\d|3[01]))?"#).unwrap()
    });
    if let Some(cap) = url_re.captures(url) {
        if let Some(day) = cap.get(3) {
            return Some(format!("{}-{}-{}", &cap[1], &cap[2], day.as_str()));
        } else {
            return Some(format!("{}-{}-01", &cap[1], &cap[2]));
        }
    }

    // 3. Polish/English text date: e.g. "24 paź 2024", "15 May 2023"
    static TEXT_DATE_RE: OnceLock<Regex> = OnceLock::new();
    let text_re = TEXT_DATE_RE.get_or_init(|| {
        Regex::new(r#"(?i)\b([0-3]?\d)\s+([a-ząćęłńóśźż]{3,10})\s+(20[12]\d)\b"#).unwrap()
    });
    if let Some(cap) = text_re.captures(snippet) {
        let day: u32 = cap[1].parse().unwrap_or(1);
        let month_str = cap[2].to_lowercase();
        let year = &cap[3];
        let m = match month_str.as_str() {
            m if m.starts_with("sty") || m.starts_with("jan") => "01",
            m if m.starts_with("lut") || m.starts_with("feb") => "02",
            m if m.starts_with("mar") => "03",
            m if m.starts_with("kwi") || m.starts_with("apr") => "04",
            m if m.starts_with("maj") || m.starts_with("may") => "05",
            m if m.starts_with("cze") || m.starts_with("jun") => "06",
            m if m.starts_with("lip") || m.starts_with("jul") => "07",
            m if m.starts_with("sie") || m.starts_with("aug") => "08",
            m if m.starts_with("wrz") || m.starts_with("sep") => "09",
            m if m.starts_with("paź") || m.starts_with("paz") || m.starts_with("oct") => "10",
            m if m.starts_with("lis") || m.starts_with("nov") => "11",
            m if m.starts_with("gru") || m.starts_with("dec") => "12",
            _ => "01",
        };
        return Some(format!("{year}-{m}-{day:02}"));
    }

    // 4. Any explicit 4-digit year in 2010-2026 range
    static YEAR_RE: OnceLock<Regex> = OnceLock::new();
    let year_re = YEAR_RE.get_or_init(|| Regex::new(r#"\b(20[12]\d)\b"#).unwrap());
    if let Some(cap) = year_re.captures(snippet).or_else(|| year_re.captures(url)) {
        return Some(format!("{}-01-01", &cap[1]));
    }

    None
}

pub fn extract_follow_ups(text: &str, topic: &str) -> Vec<String> {
    let mut follow_ups = Vec::new();
    if let Some(pos) = text.find("### Proponowane kierunki śledztwa") {
        let after = &text[pos..];
        for line in after.lines().skip(1) {
            let trimmed = line.trim();
            if trimmed.starts_with('-')
                || trimmed.starts_with('*')
                || (trimmed.len() > 3
                    && trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
                    && trimmed.contains('.'))
            {
                let clean = trimmed
                    .trim_start_matches(|c: char| {
                        c == '-' || c == '*' || c.is_ascii_digit() || c == '.' || c == ' '
                    })
                    .trim();
                if clean.len() > 5 {
                    follow_ups.push(clean.to_string());
                }
            }
            if follow_ups.len() >= 3 {
                break;
            }
        }
    }
    if follow_ups.is_empty() {
        follow_ups.push(format!("Dowody źródłowe i dokumenty: {topic}"));
        follow_ups.push(format!("Tło historyczne i początki: {topic}"));
        follow_ups.push(format!("Kluczowe postacie i instytucje: {topic}"));
    }
    follow_ups
}

fn classify_options(cfg: &Config) -> ClassifyOptions {
    ClassifyOptions {
        max_input_chars: cfg.llm.max_input_chars.max(1_000),
        max_claims: cfg.llm.max_claims.clamp(1, 50),
        max_attempts: cfg.llm.max_attempts.clamp(1, 4),
        min_confidence: 0.0,
    }
}

fn build_llm_client(cfg: &Config) -> Result<Arc<dyn bse_llm::Llm>, anyhow::Error> {
    let provider = cfg.llm.provider.trim().to_ascii_lowercase();
    match provider.as_str() {
        "embedded" | "built-in" | "builtin" | "qwen" => {
            let storage = bse_llm::ModelStorage::default();
            let preset = bse_llm::ModelCatalog::find(&cfg.llm.embedded_model)
                .unwrap_or_else(bse_llm::ModelCatalog::default_preset);
            if storage.is_installed(&preset) {
                let max_toks = (cfg.llm.max_tokens as usize).clamp(64, 250);
                let embedded = bse_llm::EmbeddedLlm::load(&storage, preset)
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?
                    .with_max_tokens(max_toks);
                Ok(Arc::new(embedded))
            } else {
                Err(anyhow::anyhow!(
                    "Wbudowany model {} nie jest jeszcze pobrany.",
                    preset.name
                ))
            }
        }
        "ollama" | "openai" => {
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
            let client = LlmClient::new(settings).map_err(|e| anyhow::anyhow!(e.to_string()))?;
            Ok(Arc::new(client))
        }
        other => Err(anyhow::anyhow!("unknown LLM provider: {other}")),
    }
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
        "embedded" | "built-in" | "builtin" | "qwen" => {
            let storage = bse_llm::ModelStorage::default();
            let preset = bse_llm::ModelCatalog::find(&cfg.llm.embedded_model)
                .unwrap_or_else(bse_llm::ModelCatalog::default_preset);
            if storage.is_installed(&preset) {
                let embedded = bse_llm::EmbeddedLlm::load(&storage, preset)
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                Ok(Arc::new(LlmClassifier::new(
                    Arc::new(embedded),
                    classify_options(cfg),
                )))
            } else {
                Ok(Arc::new(HeuristicClassifier::new(classify_options(cfg))))
            }
        }
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

    #[test]
    fn extract_date_heuristic_works() {
        assert_eq!(
            extract_date_heuristic(
                "Raport z 24 paź 2024 dotyczący schronów",
                "https://news.pl/a"
            ),
            Some("2024-10-24".to_string())
        );
        assert_eq!(
            extract_date_heuristic(
                "Wydarzenia archiwalne",
                "https://portal.pl/2023/05/12/artykul"
            ),
            Some("2023-05-12".to_string())
        );
        assert_eq!(
            extract_date_heuristic("Dokument z 2021-08-15 o podziemiach", "https://doc.org"),
            Some("2021-08-15".to_string())
        );
        assert_eq!(
            extract_date_heuristic("Opublikowano 3 dni temu w mediach", "https://news.com"),
            Some(chrono::Utc::now().format("%Y-%m-%d").to_string())
        );
    }

    #[test]
    fn extract_follow_ups_parses_sections_or_generates_defaults() {
        let text_with_section = "Analiza Krakowa.\n\n### Źródła\n1. link\n\n### Proponowane kierunki śledztwa:\n- Czakram Wawelski — dowody i archiwa\n- Tunele i schrony Nowej Huty\n- Klątwa Jagiellończyka 1973";
        let follow_ups = extract_follow_ups(text_with_section, "Kraków");
        assert_eq!(follow_ups.len(), 3);
        assert_eq!(follow_ups[0], "Czakram Wawelski — dowody i archiwa");
        assert_eq!(follow_ups[1], "Tunele i schrony Nowej Huty");
        assert_eq!(follow_ups[2], "Klątwa Jagiellończyka 1973");

        let text_without = "Zwykły tekst bez sekcji.";
        let defaults = extract_follow_ups(text_without, "Kraków");
        assert_eq!(defaults.len(), 3);
        assert!(defaults[0].contains("Kraków"));
    }
}
