//! Ekstrakcja twierdzeń i stance z pobranego tekstu.
//!
//! Dwa backendy:
//! - `LlmClassifier` — Ollama / OpenAI-compatible, strict JSON, naprawa odpowiedzi,
//! - `HeuristicClassifier` — deterministyczny fallback offline.
//!
//! Klasyfikator nie zapisuje niczego. Pipeline dodaje `CitationGate` i dopiero
//! potem trafia do evidence ledger.

mod filters;
mod heuristic;
mod llm;

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use bse_core::{ClaimKind, EntityKind, Stance};
use bse_llm::LlmError;

pub use heuristic::HeuristicClassifier;
pub use llm::{LlmClassifier, PROMPT_VERSION};

/// Wejście klasyfikatora. `text` jest pełnym, wyekstrahowanym tekstem źródła.
#[derive(Debug, Clone)]
pub struct ClassifyInput {
    pub title: String,
    pub url: String,
    pub text: String,
    pub lang: Option<String>,
}

impl ClassifyInput {
    pub fn new(
        title: impl Into<String>,
        url: impl Into<String>,
        text: impl Into<String>,
        lang: Option<String>,
    ) -> Self {
        Self {
            title: title.into(),
            url: url.into(),
            text: text.into(),
            lang,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClassifyOptions {
    pub max_input_chars: usize,
    pub max_claims: usize,
    pub max_attempts: usize,
    pub min_confidence: f32,
}

impl Default for ClassifyOptions {
    fn default() -> Self {
        Self {
            max_input_chars: 14_000,
            max_claims: 8,
            max_attempts: 2,
            min_confidence: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassifiedEntity {
    pub name: String,
    pub kind: EntityKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassifiedClaim {
    pub text: String,
    pub quote: String,
    pub stance: Stance,
    pub kind: ClaimKind,
    pub confidence: f32,
    pub entities: Vec<ClassifiedEntity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Classification {
    pub claims: Vec<ClassifiedClaim>,
    pub model: String,
    pub prompt_version: String,
    pub input_chars: usize,
    pub truncated: bool,
    pub prompt_injection_suspected: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ClassifyError {
    #[error("source text is empty")]
    EmptyInput,

    #[error(transparent)]
    Llm(#[from] LlmError),

    #[error("invalid classifier output: {0}")]
    InvalidOutput(String),

    #[error("invalid classifier configuration: {0}")]
    Config(String),

    #[error("primary classifier failed ({primary}); fallback failed: {fallback}")]
    Fallback {
        primary: &'static str,
        fallback: String,
    },
}

impl ClassifyError {
    pub fn kind(&self) -> &'static str {
        match self {
            ClassifyError::EmptyInput => "empty_input",
            ClassifyError::Llm(error) => error.kind(),
            ClassifyError::InvalidOutput(_) => "invalid_output",
            ClassifyError::Config(_) => "config",
            ClassifyError::Fallback { .. } => "fallback",
        }
    }
}

#[async_trait]
pub trait Classifier: Send + Sync {
    async fn classify(&self, input: &ClassifyInput) -> Result<Classification, ClassifyError>;
}

/// Próbuje primary, a po jego błędzie — fallback (np. API albo heurystyka).
pub struct FallbackClassifier {
    primary: Arc<dyn Classifier>,
    fallback: Arc<dyn Classifier>,
}

impl FallbackClassifier {
    pub fn new(primary: Arc<dyn Classifier>, fallback: Arc<dyn Classifier>) -> Self {
        Self { primary, fallback }
    }
}

#[async_trait]
impl Classifier for FallbackClassifier {
    async fn classify(&self, input: &ClassifyInput) -> Result<Classification, ClassifyError> {
        match self.primary.classify(input).await {
            Ok(classification) => Ok(classification),
            Err(primary_error) => match self.fallback.classify(input).await {
                Ok(mut classification) => {
                    classification
                        .warnings
                        .insert(0, format!("primary_failed:{}", primary_error.kind()));
                    Ok(classification)
                }
                Err(fallback_error) => Err(ClassifyError::Fallback {
                    primary: primary_error.kind(),
                    fallback: format!("{}: {fallback_error}", fallback_error.kind()),
                }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeClassifier {
        result: Result<Classification, ClassifyError>,
    }

    #[async_trait]
    impl Classifier for FakeClassifier {
        async fn classify(&self, _input: &ClassifyInput) -> Result<Classification, ClassifyError> {
            match &self.result {
                Ok(classification) => Ok(classification.clone()),
                Err(ClassifyError::EmptyInput) => Err(ClassifyError::EmptyInput),
                Err(ClassifyError::Config(message)) => Err(ClassifyError::Config(message.clone())),
                Err(_) => Err(ClassifyError::InvalidOutput("fake".to_string())),
            }
        }
    }

    fn classification(warning: &str) -> Classification {
        Classification {
            claims: vec![],
            model: "fake".to_string(),
            prompt_version: "fake".to_string(),
            input_chars: 0,
            truncated: false,
            prompt_injection_suspected: false,
            warnings: vec![warning.to_string()],
        }
    }

    #[tokio::test]
    async fn fallback_marks_primary_failure() {
        let primary = Arc::new(FakeClassifier {
            result: Err(ClassifyError::Config("down".to_string())),
        });
        let fallback = Arc::new(FakeClassifier {
            result: Ok(classification("fallback")),
        });
        let classifier = FallbackClassifier::new(primary, fallback);
        let result = classifier
            .classify(&ClassifyInput::new("t", "u", "text", None))
            .await
            .unwrap();
        assert_eq!(result.warnings[0], "primary_failed:config");
    }
}
