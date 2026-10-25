//! Transport LLM: lokalna Ollama i endpointy OpenAI-compatible.
//!
//! Zasady:
//! - żadnego proxy środowiskowego (lokalny model ma działać bez pośredników),
//! - `http` tylko dla loopbacku; zdalne endpointy muszą być `https`,
//! - klucz API nigdy nie trafia do logów ani do błędów,
//! - odpowiedź jest ograniczona rozmiarem, a błędy serwera są skracane.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::redirect::Policy;
use reqwest::Url;
use serde_json::{json, Value};
use thiserror::Error;

const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_ERROR_CHARS: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProvider {
    Ollama,
    OpenAi,
}

impl LlmProvider {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "ollama" | "local" => Some(LlmProvider::Ollama),
            "openai" | "openai-compatible" | "openai_compatible" | "api" => {
                Some(LlmProvider::OpenAi)
            }
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            LlmProvider::Ollama => "ollama",
            LlmProvider::OpenAi => "openai",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LlmSettings {
    pub provider: LlmProvider,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub timeout: Duration,
    pub connect_timeout: Duration,
    pub temperature: f32,
    pub max_tokens: u32,
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            provider: LlmProvider::Ollama,
            base_url: "http://127.0.0.1:11434".to_string(),
            model: "llama3.1:8b".to_string(),
            api_key: None,
            timeout: Duration::from_secs(60),
            connect_timeout: Duration::from_secs(5),
            temperature: 0.1,
            max_tokens: 1200,
        }
    }
}

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("invalid LLM configuration: {0}")]
    Config(String),

    #[error("invalid LLM endpoint: {0}")]
    InvalidEndpoint(String),

    #[error("LLM request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("LLM endpoint returned HTTP {status}: {body}")]
    Status { status: u16, body: String },

    #[error("LLM API error: {0}")]
    Api(String),

    #[error("cannot decode LLM response: {0}")]
    Decode(String),

    #[error("LLM response is empty")]
    Empty,

    #[error("LLM response exceeds {0} bytes")]
    TooLarge(usize),
}

impl LlmError {
    /// Stabilny kod do raportów/audytu; bez treści źródła i bez klucza.
    pub fn kind(&self) -> &'static str {
        match self {
            LlmError::Config(_) => "config",
            LlmError::InvalidEndpoint(_) => "invalid_endpoint",
            LlmError::Http(_) => "http",
            LlmError::Status { .. } => "status",
            LlmError::Api(_) => "api",
            LlmError::Decode(_) => "decode",
            LlmError::Empty => "empty",
            LlmError::TooLarge(_) => "too_large",
        }
    }
}

#[async_trait]
pub trait Llm: Send + Sync {
    fn model(&self) -> &str;
    async fn complete(&self, system: &str, user: &str) -> Result<String, LlmError>;
    async fn complete_text(&self, system: &str, user: &str) -> Result<String, LlmError> {
        self.complete(system, user).await
    }
}

pub struct LlmClient {
    http: reqwest::Client,
    settings: LlmSettings,
    endpoint: String,
}

impl LlmClient {
    pub fn new(settings: LlmSettings) -> Result<Self, LlmError> {
        validate_settings(&settings)?;
        let http = reqwest::Client::builder()
            .no_proxy()
            // Endpoint jest jawny; redirect mógłby wynieść prompt/klucz pod inny host.
            .redirect(Policy::none())
            .timeout(settings.timeout)
            .connect_timeout(settings.connect_timeout)
            .user_agent("BALD/0.1 (LLM client)")
            .build()?;
        let endpoint = endpoint_for(settings.provider, &settings.base_url);
        Ok(Self {
            http,
            settings,
            endpoint,
        })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    async fn send(&self, body: &Value) -> Result<String, LlmError> {
        let mut request = self.http.post(&self.endpoint).json(body);
        if let Some(key) = self
            .settings
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
        {
            request = request.bearer_auth(key);
        }
        let response = request.send().await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(LlmError::TooLarge(MAX_RESPONSE_BYTES));
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        if !status.is_success() {
            return Err(LlmError::Status {
                status: status.as_u16(),
                body: api_error_message(&text),
            });
        }
        Ok(text)
    }
}

#[async_trait]
impl Llm for LlmClient {
    fn model(&self) -> &str {
        &self.settings.model
    }

    async fn complete(&self, system: &str, user: &str) -> Result<String, LlmError> {
        if system.trim().is_empty() || user.trim().is_empty() {
            return Err(LlmError::Config("empty prompt".to_string()));
        }

        let messages = json!([
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ]);

        let mut body = match self.settings.provider {
            LlmProvider::Ollama => json!({
                "model": self.settings.model,
                "messages": messages,
                "stream": false,
                "format": "json",
                "options": {
                    "temperature": self.settings.temperature,
                    "num_predict": self.settings.max_tokens
                }
            }),
            LlmProvider::OpenAi => json!({
                "model": self.settings.model,
                "messages": messages,
                "temperature": self.settings.temperature,
                "max_tokens": self.settings.max_tokens,
                "response_format": {"type": "json_object"}
            }),
        };

        let first = self.send(&body).await;
        let text = match first {
            Ok(text) => text,
            // Część lokalnych serwerów OpenAI-compatible nie zna response_format.
            Err(LlmError::Status {
                status: 400,
                body: message,
            }) if self.settings.provider == LlmProvider::OpenAi
                && message.contains("response_format") =>
            {
                if let Some(object) = body.as_object_mut() {
                    object.remove("response_format");
                }
                self.send(&body).await?
            }
            Err(error) => return Err(error),
        };

        let content = match self.settings.provider {
            LlmProvider::Ollama => parse_ollama_content(&text)?,
            LlmProvider::OpenAi => parse_openai_content(&text)?,
        };
        if content.trim().is_empty() {
            return Err(LlmError::Empty);
        }
        Ok(content)
    }

    /// Odpowiedź w postaci swobodnego tekstu (np. czat, asystent), bez wymuszania schematu JSON.
    async fn complete_text(&self, system: &str, user: &str) -> Result<String, LlmError> {
        if system.trim().is_empty() || user.trim().is_empty() {
            return Err(LlmError::Config("empty prompt".to_string()));
        }

        let messages = json!([
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ]);

        let body = match self.settings.provider {
            LlmProvider::Ollama => json!({
                "model": self.settings.model,
                "messages": messages,
                "stream": false,
                "options": {
                    "temperature": self.settings.temperature,
                    "num_predict": self.settings.max_tokens
                }
            }),
            LlmProvider::OpenAi => json!({
                "model": self.settings.model,
                "messages": messages,
                "temperature": self.settings.temperature,
                "max_tokens": self.settings.max_tokens
            }),
        };

        let text = self.send(&body).await?;
        let content = match self.settings.provider {
            LlmProvider::Ollama => parse_ollama_content(&text)?,
            LlmProvider::OpenAi => parse_openai_content(&text)?,
        };
        if content.trim().is_empty() {
            return Err(LlmError::Empty);
        }
        Ok(content)
    }
}

fn validate_settings(settings: &LlmSettings) -> Result<(), LlmError> {
    if settings.model.trim().is_empty() {
        return Err(LlmError::Config("model is empty".to_string()));
    }
    if !(0.0..=2.0).contains(&settings.temperature) {
        return Err(LlmError::Config(
            "temperature must be between 0.0 and 2.0".to_string(),
        ));
    }
    if settings.timeout.is_zero() || settings.connect_timeout.is_zero() {
        return Err(LlmError::Config(
            "timeouts must be greater than zero".to_string(),
        ));
    }

    let url = Url::parse(settings.base_url.trim())
        .map_err(|error| LlmError::InvalidEndpoint(error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(LlmError::InvalidEndpoint(
            "only http/https are allowed".to_string(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(LlmError::InvalidEndpoint(
            "credentials in URL are not allowed".to_string(),
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(LlmError::InvalidEndpoint(
            "query/fragment in base URL are not allowed".to_string(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| LlmError::InvalidEndpoint("missing host".to_string()))?;
    if url.scheme() == "http" && !is_loopback(host) {
        return Err(LlmError::InvalidEndpoint(
            "plain http is allowed only for loopback".to_string(),
        ));
    }
    Ok(())
}

fn endpoint_for(provider: LlmProvider, base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    match provider {
        LlmProvider::Ollama => {
            if base.ends_with("/api/chat") {
                base.to_string()
            } else {
                format!("{base}/api/chat")
            }
        }
        LlmProvider::OpenAi => {
            if base.ends_with("/chat/completions") {
                base.to_string()
            } else {
                format!("{base}/chat/completions")
            }
        }
    }
}

fn is_loopback(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
}

fn parse_ollama_content(raw: &str) -> Result<String, LlmError> {
    let value: Value =
        serde_json::from_str(raw).map_err(|error| LlmError::Decode(error.to_string()))?;
    if let Some(text) = value.pointer("/message/content").and_then(Value::as_str) {
        return Ok(text.to_string());
    }
    if let Some(text) = value.get("response").and_then(Value::as_str) {
        return Ok(text.to_string());
    }
    if let Some(error) = value.pointer("/error").and_then(Value::as_str) {
        return Err(LlmError::Api(truncate(&redact_secrets(error))));
    }
    Err(LlmError::Decode(
        "missing message.content/response".to_string(),
    ))
}

fn parse_openai_content(raw: &str) -> Result<String, LlmError> {
    let value: Value =
        serde_json::from_str(raw).map_err(|error| LlmError::Decode(error.to_string()))?;
    let content = value
        .pointer("/choices/0/message/content")
        .ok_or_else(|| LlmError::Decode("missing choices[0].message.content".to_string()))?;
    if let Some(text) = content.as_str() {
        return Ok(text.to_string());
    }
    // Niektóre bramki zwracają tablicę części tekstowych.
    if let Some(parts) = content.as_array() {
        let text = parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("");
        if !text.is_empty() {
            return Ok(text);
        }
    }
    Err(LlmError::Decode("message.content is not text".to_string()))
}

fn api_error_message(raw: &str) -> String {
    let value: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
    if let Some(message) = value
        .pointer("/error/message")
        .and_then(Value::as_str)
        .or_else(|| value.get("message").and_then(Value::as_str))
    {
        return truncate(&redact_secrets(message));
    }
    truncate(&redact_secrets(raw))
}

fn redact_secrets(raw: &str) -> String {
    let mut out = Vec::new();
    let mut redact_next = false;
    for token in raw.split_whitespace() {
        if redact_next {
            out.push("[redacted]");
            redact_next = false;
            continue;
        }
        let lower = token.to_ascii_lowercase();
        if lower == "bearer" {
            out.push(token);
            redact_next = true;
        } else if lower.starts_with("sk-") || lower.starts_with("bearer") {
            out.push("[redacted]");
        } else {
            out.push(token);
        }
    }
    out.join(" ")
}

fn truncate(raw: &str) -> String {
    raw.chars().take(MAX_ERROR_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(provider: LlmProvider, base_url: &str) -> LlmSettings {
        LlmSettings {
            provider,
            base_url: base_url.to_string(),
            ..LlmSettings::default()
        }
    }

    #[test]
    fn accepts_https_and_loopback_http() {
        assert!(
            validate_settings(&settings(LlmProvider::OpenAi, "https://api.example.com/v1")).is_ok()
        );
        assert!(
            validate_settings(&settings(LlmProvider::Ollama, "http://127.0.0.1:11434")).is_ok()
        );
        assert!(
            validate_settings(&settings(LlmProvider::Ollama, "http://localhost:11434")).is_ok()
        );
        assert!(validate_settings(&settings(LlmProvider::Ollama, "http://[::1]:11434")).is_ok());
    }

    #[test]
    fn rejects_insecure_remote_and_credentials() {
        assert!(matches!(
            validate_settings(&settings(LlmProvider::OpenAi, "http://api.example.com/v1")),
            Err(LlmError::InvalidEndpoint(_))
        ));
        assert!(matches!(
            validate_settings(&settings(
                LlmProvider::OpenAi,
                "https://user:pass@example.com/v1"
            )),
            Err(LlmError::InvalidEndpoint(_))
        ));
    }

    #[test]
    fn endpoint_joining_is_idempotent() {
        assert_eq!(
            endpoint_for(LlmProvider::Ollama, "http://127.0.0.1:11434"),
            "http://127.0.0.1:11434/api/chat"
        );
        assert_eq!(
            endpoint_for(LlmProvider::Ollama, "http://127.0.0.1:11434/api/chat"),
            "http://127.0.0.1:11434/api/chat"
        );
        assert_eq!(
            endpoint_for(LlmProvider::OpenAi, "https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn parses_ollama_and_openai_payloads() {
        let ollama = r#"{"message":{"content":"{\"claims\":[]}"}}"#;
        assert_eq!(parse_ollama_content(ollama).unwrap(), "{\"claims\":[]}");

        let openai = r#"{"choices":[{"message":{"content":"{\"claims\":[]}"}}]}"#;
        assert_eq!(parse_openai_content(openai).unwrap(), "{\"claims\":[]}");

        let parts = r#"{"choices":[{"message":{"content":[{"type":"text","text":"{\"a\":1}"}]}}]}"#;
        assert_eq!(parse_openai_content(parts).unwrap(), "{\"a\":1}");
    }

    #[test]
    fn api_error_is_redacted() {
        let raw = r#"{"error":{"message":"bad key sk-secret"}}"#;
        assert_eq!(api_error_message(raw), "bad key [redacted]");
        assert_eq!(api_error_message("plain error"), "plain error");
        assert_eq!(
            api_error_message("Authorization: Bearer abc.def.ghi"),
            "Authorization: Bearer [redacted]"
        );
    }
}
