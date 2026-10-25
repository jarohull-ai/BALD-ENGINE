use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::Lang;
use crate::gate::GateVerdict;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub text: String,
    pub lang: Lang,
    pub limit: usize,
}

impl SearchQuery {
    pub fn new(text: impl Into<String>, lang: Lang, limit: usize) -> Self {
        Self {
            text: text.into(),
            lang,
            limit,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
    /// Pozycja w wynikach providera, 1-based.
    pub rank: usize,
    pub provider: String,
    pub published: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub web: bool,
    pub news: bool,
    pub archive: bool,
    pub factcheck: bool,
    pub render: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            web: true,
            news: false,
            archive: false,
            factcheck: false,
            render: false,
        }
    }
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider is not configured: {0}")]
    Config(String),

    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("rate limited by provider")]
    RateLimited,

    #[error("request timed out")]
    Timeout,

    #[error("http error: {0}")]
    Http(String),

    #[error("could not decode provider response: {0}")]
    Decode(String),

    #[error("provider unavailable: {0}")]
    Unavailable(String),
}

impl ProviderError {
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            ProviderError::RateLimited | ProviderError::Timeout | ProviderError::Http(_)
        )
    }
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<RawHit>, ProviderError>;
}

/// Publiczny, lekki status providera w raporcie (bez surowych wyników).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub provider: String,
    pub ok: bool,
    pub hits: usize,
    pub latency_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedHit {
    pub title: String,
    pub url: String,
    pub canonical_url: String,
    pub snippet: String,
    pub score: f32,
    pub providers: Vec<String>,
    pub best_rank: usize,
    pub published: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchReport {
    pub query: String,
    pub lang: Lang,
    pub gate: GateVerdict,
    pub hits: Vec<RankedHit>,
    pub providers: Vec<ProviderStatus>,
    pub latency_ms: u64,
    pub warnings: Vec<String>,
    pub stored_run_id: Option<String>,
}
