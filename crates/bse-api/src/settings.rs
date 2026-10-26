//! Bezpieczny widok i walidacja ustawień lokalnych.
//!
//! Klucze API nigdy nie są zwracane do UI. Widok zawiera wyłącznie informację,
//! czy klucz istnieje i czy pochodzi z env, czy z pliku 0600.

use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::{Host, Url};

use bse_config::Config;

const ENV_SETTING_NAMES: &[&str] = &[
    "BALD_SEARXNG_ENABLED",
    "BALD_SEARXNG_URL",
    "BALD_BRAVE_ENABLED",
    "BRAVE_API_KEY",
    "BALD_LLM_API_KEY",
    "BALD_LLM_FALLBACK_API_KEY",
    "BALD_LLM_PROVIDER",
    "BALD_LLM_FALLBACK_PROVIDER",
    "BALD_LLM_BASE_URL",
    "BALD_LLM_MODEL",
    "BALD_LLM_API_KEY_ENV",
    "BALD_LLM_FALLBACK_BASE_URL",
    "BALD_LLM_FALLBACK_MODEL",
    "BALD_LLM_FALLBACK_API_KEY_ENV",
    "BALD_LLM_TIMEOUT_SECS",
    "BALD_LLM_CONNECT_TIMEOUT_SECS",
    "BALD_LLM_TEMPERATURE",
    "BALD_LLM_MAX_TOKENS",
    "BALD_LLM_MAX_INPUT_CHARS",
    "BALD_LLM_MAX_CLAIMS",
    "BALD_LLM_MAX_ATTEMPTS",
];

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("invalid settings: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub config_path: String,
    pub language: String,
    pub searxng: SearxngView,
    pub brave: BraveView,
    pub llm: LlmView,
    pub scout: ScoutView,
    pub env_overrides: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoutView {
    pub enabled: bool,
    pub interval_days: u32,
    pub last_run: Option<String>,
    pub in_trial: bool,
    pub trial_model: Option<String>,
    pub previous_model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearxngView {
    pub enabled: bool,
    pub base_url: String,
    pub language: Option<String>,
    pub safesearch: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BraveView {
    pub enabled: bool,
    pub country: Option<String>,
    pub api_key_configured: bool,
    pub api_key_source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmView {
    pub provider: String,
    pub fallback_provider: String,
    pub base_url: String,
    pub model: String,
    pub embedded_model: String,
    pub embedded_installed: bool,
    pub api_key_env: String,
    pub api_key_configured: bool,
    pub api_key_source: String,
    pub fallback_base_url: String,
    pub fallback_model: String,
    pub fallback_api_key_env: String,
    pub fallback_api_key_configured: bool,
    pub fallback_api_key_source: String,
    pub timeout_secs: u64,
    pub connect_timeout_secs: u64,
    pub temperature: f32,
    pub max_tokens: u32,
    pub max_input_chars: usize,
    pub max_claims: usize,
    pub max_attempts: usize,
}

/// Płaski kontrakt UI. Pola sekretów są jednorazowe: pusty string = brak zmiany.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsUpdate {
    pub language: Option<String>,
    pub searxng_enabled: Option<bool>,
    pub searxng_base_url: Option<String>,
    pub searxng_language: Option<String>,
    pub searxng_safesearch: Option<u8>,
    pub brave_enabled: Option<bool>,
    pub brave_country: Option<String>,
    pub brave_api_key: Option<String>,
    pub clear_brave_api_key: Option<bool>,
    pub llm_provider: Option<String>,
    pub llm_base_url: Option<String>,
    pub llm_model: Option<String>,
    pub embedded_model: Option<String>,
    pub llm_api_key_env: Option<String>,
    pub llm_api_key: Option<String>,
    pub clear_llm_api_key: Option<bool>,
    pub fallback_provider: Option<String>,
    pub fallback_base_url: Option<String>,
    pub fallback_model: Option<String>,
    pub fallback_api_key_env: Option<String>,
    pub fallback_api_key: Option<String>,
    pub clear_fallback_api_key: Option<bool>,
    pub timeout_secs: Option<u64>,
    pub connect_timeout_secs: Option<u64>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub max_input_chars: Option<usize>,
    pub max_claims: Option<usize>,
    pub max_attempts: Option<usize>,
    pub scout_enabled: Option<bool>,
    pub scout_interval_days: Option<u32>,
}

pub fn view(config: &Config, config_path: &Path) -> SettingsView {
    let brave_source = brave_key_source(config);
    let llm_source = llm_key_source(config);
    let fallback_source = fallback_key_source(config);

    let storage = bse_llm::ModelStorage::default();
    let preset = bse_llm::ModelCatalog::find(&config.llm.embedded_model)
        .unwrap_or_else(bse_llm::ModelCatalog::default_preset);
    let embedded_installed = storage.is_installed(&preset);

    SettingsView {
        config_path: config_path.display().to_string(),
        language: config.app.language.clone(),
        searxng: SearxngView {
            enabled: config.providers.searxng.enabled,
            base_url: config.providers.searxng.base_url.clone(),
            language: config.providers.searxng.language.clone(),
            safesearch: config.providers.searxng.safesearch,
        },
        brave: BraveView {
            enabled: config.providers.brave.enabled,
            country: config.providers.brave.country.clone(),
            api_key_configured: brave_source != "none",
            api_key_source: brave_source.to_string(),
        },
        llm: LlmView {
            provider: config.llm.provider.clone(),
            fallback_provider: config.llm.fallback_provider.clone(),
            base_url: config.llm.base_url.clone(),
            model: config.llm.model.clone(),
            embedded_model: config.llm.embedded_model.clone(),
            embedded_installed,
            api_key_env: config.llm.api_key_env.clone(),
            api_key_configured: llm_source != "none",
            api_key_source: llm_source.to_string(),
            fallback_base_url: config.llm.fallback_base_url.clone(),
            fallback_model: config.llm.fallback_model.clone(),
            fallback_api_key_env: config.llm.fallback_api_key_env.clone(),
            fallback_api_key_configured: fallback_source != "none",
            fallback_api_key_source: fallback_source.to_string(),
            timeout_secs: config.llm.timeout_secs,
            connect_timeout_secs: config.llm.connect_timeout_secs,
            temperature: config.llm.temperature,
            max_tokens: config.llm.max_tokens,
            max_input_chars: config.llm.max_input_chars,
            max_claims: config.llm.max_claims,
            max_attempts: config.llm.max_attempts,
        },
        scout: ScoutView {
            enabled: config.scout.enabled,
            interval_days: config.scout.interval_days,
            last_run: config.scout.last_run.clone(),
            in_trial: config.scout.in_trial,
            trial_model: config.scout.trial_model.clone(),
            previous_model: config.scout.previous_model.clone(),
        },
        env_overrides: env_overrides(config),
    }
}

/// Nakłada zmiany na kopię konfiguracji. Wywołujący zapisuje dopiero po sukcesie.
pub fn apply_update(config: &mut Config, update: SettingsUpdate) -> Result<(), SettingsError> {
    if let Some(language) = update.language {
        let language = language.trim().to_ascii_lowercase();
        if !matches!(language.as_str(), "pl" | "en") {
            return Err(invalid("language must be pl or en"));
        }
        config.app.language = language;
    }

    if let Some(enabled) = update.searxng_enabled {
        config.providers.searxng.enabled = enabled;
    }
    if let Some(base_url) = update.searxng_base_url {
        config.providers.searxng.base_url = endpoint(&base_url, "searxng.base_url")?;
    }
    if let Some(language) = update.searxng_language {
        let language = language.trim().to_ascii_lowercase();
        config.providers.searxng.language = match language.as_str() {
            "" => None,
            "pl" | "en" => Some(language),
            _ => return Err(invalid("searxng.language must be empty, pl or en")),
        };
    }
    if let Some(safesearch) = update.searxng_safesearch {
        if safesearch > 2 {
            return Err(invalid("searxng.safesearch must be 0, 1 or 2"));
        }
        config.providers.searxng.safesearch = safesearch;
    }

    if let Some(enabled) = update.brave_enabled {
        config.providers.brave.enabled = enabled;
    }
    if let Some(country) = update.brave_country {
        let country = country.trim().to_ascii_lowercase();
        config.providers.brave.country = match country.as_str() {
            "" => None,
            value if value.len() == 2 && value.chars().all(|c| c.is_ascii_alphabetic()) => {
                Some(value.to_string())
            }
            _ => return Err(invalid("brave.country must be a two-letter country code")),
        };
    }
    if let Some(key) = update.brave_api_key {
        if update.clear_brave_api_key == Some(true) {
            return Err(invalid("cannot set and clear brave.api_key in one request"));
        }
        if let Some(key) = secret(&key, "brave.api_key")? {
            config.providers.brave.api_key = Some(key);
        }
    }
    if update.clear_brave_api_key == Some(true) {
        config.providers.brave.api_key = None;
    }

    if let Some(provider) = update.llm_provider {
        let provider = provider.trim().to_ascii_lowercase();
        if !matches!(provider.as_str(), "embedded" | "built-in" | "ollama" | "openai" | "none") {
            return Err(invalid("llm.provider must be embedded, ollama, openai or none"));
        }
        config.llm.provider = provider;
    }
    if let Some(embedded_model) = update.embedded_model {
        config.llm.embedded_model = embedded_model.trim().to_string();
    }
    if let Some(base_url) = update.llm_base_url {
        config.llm.base_url = endpoint(&base_url, "llm.base_url")?;
    }
    if let Some(model) = update.llm_model {
        config.llm.model = model_name(&model, "llm.model")?;
    }
    if let Some(name) = update.llm_api_key_env {
        config.llm.api_key_env = env_name(&name, "llm.api_key_env")?;
    }
    if let Some(key) = update.llm_api_key {
        if update.clear_llm_api_key == Some(true) {
            return Err(invalid("cannot set and clear llm.api_key in one request"));
        }
        if let Some(key) = secret(&key, "llm.api_key")? {
            config.llm.api_key = Some(key);
        }
    }
    if update.clear_llm_api_key == Some(true) {
        config.llm.api_key = None;
    }

    if let Some(provider) = update.fallback_provider {
        let provider = provider.trim().to_ascii_lowercase();
        if !matches!(
            provider.as_str(),
            "none" | "heuristic" | "ollama" | "openai"
        ) {
            return Err(invalid(
                "llm.fallback_provider must be none, heuristic, ollama or openai",
            ));
        }
        config.llm.fallback_provider = provider;
    }
    if let Some(base_url) = update.fallback_base_url {
        config.llm.fallback_base_url = endpoint(&base_url, "llm.fallback_base_url")?;
    }
    if let Some(model) = update.fallback_model {
        config.llm.fallback_model = model_name(&model, "llm.fallback_model")?;
    }
    if let Some(name) = update.fallback_api_key_env {
        config.llm.fallback_api_key_env = env_name(&name, "llm.fallback_api_key_env")?;
    }
    if let Some(key) = update.fallback_api_key {
        if update.clear_fallback_api_key == Some(true) {
            return Err(invalid(
                "cannot set and clear llm.fallback_api_key in one request",
            ));
        }
        if let Some(key) = secret(&key, "llm.fallback_api_key")? {
            config.llm.fallback_api_key = Some(key);
        }
    }
    if update.clear_fallback_api_key == Some(true) {
        config.llm.fallback_api_key = None;
    }

    if let Some(timeout) = update.timeout_secs {
        if !(1..=300).contains(&timeout) {
            return Err(invalid("llm.timeout_secs must be between 1 and 300"));
        }
        config.llm.timeout_secs = timeout;
    }
    if let Some(timeout) = update.connect_timeout_secs {
        if !(1..=60).contains(&timeout) {
            return Err(invalid("llm.connect_timeout_secs must be between 1 and 60"));
        }
        config.llm.connect_timeout_secs = timeout;
    }
    if config.llm.connect_timeout_secs > config.llm.timeout_secs {
        return Err(invalid(
            "llm.connect_timeout_secs cannot exceed llm.timeout_secs",
        ));
    }
    if let Some(temperature) = update.temperature {
        if !temperature.is_finite() || !(0.0..=1.0).contains(&temperature) {
            return Err(invalid("llm.temperature must be between 0 and 1"));
        }
        config.llm.temperature = temperature;
    }
    if let Some(tokens) = update.max_tokens {
        if !(64..=8192).contains(&tokens) {
            return Err(invalid("llm.max_tokens must be between 64 and 8192"));
        }
        config.llm.max_tokens = tokens;
    }
    if let Some(chars) = update.max_input_chars {
        if !(1_000..=100_000).contains(&chars) {
            return Err(invalid(
                "llm.max_input_chars must be between 1000 and 100000",
            ));
        }
        config.llm.max_input_chars = chars;
    }
    if let Some(claims) = update.max_claims {
        if !(1..=50).contains(&claims) {
            return Err(invalid("llm.max_claims must be between 1 and 50"));
        }
        config.llm.max_claims = claims;
    }
    if let Some(attempts) = update.max_attempts {
        if !(1..=4).contains(&attempts) {
            return Err(invalid("llm.max_attempts must be between 1 and 4"));
        }
        config.llm.max_attempts = attempts;
    }

    if let Some(enabled) = update.scout_enabled {
        config.scout.enabled = enabled;
    }
    if let Some(interval) = update.scout_interval_days {
        if ![7, 14, 30].contains(&interval) {
            return Err(invalid("scout.interval_days must be 7, 14 or 30"));
        }
        config.scout.interval_days = interval;
    }

    let primary = config.llm.provider.trim().to_ascii_lowercase();
    let fallback = config.llm.fallback_provider.trim().to_ascii_lowercase();
    if matches!(primary.as_str(), "ollama" | "openai") && config.llm.model.trim().is_empty() {
        return Err(invalid("llm.model is required for the primary provider"));
    }
    if matches!(fallback.as_str(), "ollama" | "openai")
        && config.llm.fallback_model.trim().is_empty()
    {
        return Err(invalid(
            "llm.fallback_model is required for the fallback provider",
        ));
    }

    Ok(())
}

fn invalid(message: &str) -> SettingsError {
    SettingsError::Invalid(message.to_string())
}

fn endpoint(raw: &str, field: &str) -> Result<String, SettingsError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(invalid(&format!("{field} cannot be empty")));
    }
    if raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(invalid(&format!(
            "{field} contains whitespace or control chars"
        )));
    }
    let url = Url::parse(raw).map_err(|_| invalid(&format!("{field} is not a valid URL")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid(&format!("{field} must use http or https")));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid(&format!("{field} cannot contain credentials")));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(invalid(&format!(
            "{field} cannot contain a query or fragment"
        )));
    }
    let Some(host) = url.host() else {
        return Err(invalid(&format!("{field} must contain a host")));
    };
    if host.to_string().to_ascii_lowercase().ends_with(".onion") {
        return Err(invalid("Tor/onion endpoints are deferred to v1.1"));
    }
    if url.scheme() == "http" {
        let loopback = match host {
            Host::Domain(domain) => domain.eq_ignore_ascii_case("localhost"),
            Host::Ipv4(ip) => ip.is_loopback(),
            Host::Ipv6(ip) => ip.is_loopback(),
        };
        if !loopback {
            return Err(invalid(&format!(
                "{field}: plain HTTP is allowed only for loopback"
            )));
        }
    }
    Ok(raw.trim_end_matches('/').to_string())
}

fn model_name(raw: &str, field: &str) -> Result<String, SettingsError> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(invalid(&format!("{field} cannot be empty")));
    }
    if value.chars().count() > 128 || value.chars().any(char::is_control) {
        return Err(invalid(&format!("{field} is invalid")));
    }
    Ok(value.to_string())
}

fn env_name(raw: &str, field: &str) -> Result<String, SettingsError> {
    let value = raw.trim();
    if value.is_empty() {
        return Ok(String::new());
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Ok(String::new());
    };
    if !(first.is_ascii_alphabetic() || first == '_')
        || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        || value.len() > 64
    {
        return Err(invalid(&format!("{field} is not a valid environment name")));
    }
    Ok(value.to_string())
}

fn secret(raw: &str, field: &str) -> Result<Option<String>, SettingsError> {
    let value = raw.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > 4096 || value.chars().any(char::is_control) {
        return Err(invalid(&format!("{field} is invalid")));
    }
    Ok(Some(value.to_string()))
}

fn read_env(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn brave_key_source(config: &Config) -> &'static str {
    if read_env("BRAVE_API_KEY") {
        return "env";
    }
    if config
        .providers
        .brave
        .api_key
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        return "config";
    }
    "none"
}

fn llm_key_source(config: &Config) -> &'static str {
    if read_env("BALD_LLM_API_KEY") || read_env(&config.llm.api_key_env) {
        return "env";
    }
    if config
        .llm
        .api_key
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        return "config";
    }
    "none"
}

fn fallback_key_source(config: &Config) -> &'static str {
    if read_env("BALD_LLM_FALLBACK_API_KEY") || read_env(&config.llm.fallback_api_key_env) {
        return "env";
    }
    if config
        .llm
        .fallback_api_key
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        return "config";
    }
    "none"
}

fn env_overrides(config: &Config) -> Vec<String> {
    let dynamic = [
        config.llm.api_key_env.as_str(),
        config.llm.fallback_api_key_env.as_str(),
    ];
    let mut names = ENV_SETTING_NAMES
        .iter()
        .copied()
        .chain(dynamic)
        .filter(|name| !name.trim().is_empty() && read_env(name))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        let mut config = Config::default();
        config.llm.api_key_env = "BALD_TEST_UNSET_KEY".to_string();
        config.llm.fallback_api_key_env = "BALD_TEST_UNSET_FALLBACK".to_string();
        config
    }

    #[test]
    fn rejects_remote_plain_http() {
        let mut config = config();
        let update = SettingsUpdate {
            llm_base_url: Some("http://example.com/v1".to_string()),
            ..SettingsUpdate::default()
        };
        assert!(apply_update(&mut config, update).is_err());
    }

    #[test]
    fn accepts_loopback_plain_http_and_https_remote() {
        let mut config = config();
        apply_update(
            &mut config,
            SettingsUpdate {
                llm_base_url: Some("http://127.0.0.1:11434/".to_string()),
                ..SettingsUpdate::default()
            },
        )
        .expect("loopback http");
        assert_eq!(config.llm.base_url, "http://127.0.0.1:11434");

        apply_update(
            &mut config,
            SettingsUpdate {
                llm_base_url: Some("https://api.openai.com/v1".to_string()),
                ..SettingsUpdate::default()
            },
        )
        .expect("remote https");
    }

    #[test]
    fn keys_are_never_exposed_in_view() {
        let mut config = config();
        config.providers.brave.api_key = Some("super-secret".to_string());
        config.llm.api_key = Some("llm-secret".to_string());
        let view = view(&config, Path::new("/tmp/config.toml"));
        let json = serde_json::to_string(&view).expect("serialize");
        assert!(!json.contains("super-secret"));
        assert!(!json.contains("llm-secret"));
        assert!(view.brave.api_key_configured);
        assert!(view.llm.api_key_configured);
    }

    #[test]
    fn clear_and_set_conflict_is_rejected() {
        let mut config = config();
        let update = SettingsUpdate {
            brave_api_key: Some("new".to_string()),
            clear_brave_api_key: Some(true),
            ..SettingsUpdate::default()
        };
        assert!(apply_update(&mut config, update).is_err());
    }

    #[test]
    fn validates_numeric_ranges() {
        let mut config = config();
        let update = SettingsUpdate {
            timeout_secs: Some(0),
            ..SettingsUpdate::default()
        };
        assert!(apply_update(&mut config, update).is_err());
    }
}
