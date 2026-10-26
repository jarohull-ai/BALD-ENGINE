//! Konfiguracja BALD.
//!
//! Priorytet: wartości domyślne → plik TOML → zmienne środowiskowe.
//! Klucze API mogą być w pliku (tryb lokalny, uprawnienia 0600), ale
//! rekomendowane są zmienne środowiskowe; w kolejnej iteracji dojdzie keyring.

use std::path::{Path, PathBuf};

use directories::{BaseDirs, ProjectDirs};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("cannot read config {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot write config {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid config {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("cannot determine home/config directory")]
    NoHome,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub app: AppConfig,
    pub storage: StorageConfig,
    pub fetch: FetchConfig,
    pub security: SecurityConfig,
    pub transport: TransportConfig,
    pub llm: LlmConfig,
    pub providers: ProvidersConfig,
    pub scout: ScoutConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// `local-first` | `hybrid` | `cloud` — na razie tylko etykieta profilu.
    pub profile: String,
    /// Domyślny język UI i zapytań: `pl` | `en`.
    pub language: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            profile: "local-first".to_string(),
            language: "pl".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    /// `sqlite` (v1). `postgres` będzie opcją feature `server`.
    pub backend: String,
    pub path: String,
    /// Katalog indeksu Tantivy.
    pub index_path: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        let data_dir = default_data_dir();
        Self {
            backend: "sqlite".to_string(),
            path: data_dir.join("bald.db").to_string_lossy().to_string(),
            index_path: data_dir.join("index").to_string_lossy().to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FetchConfig {
    /// Cache wyekstrahowanych dokumentów (nie surowego HTML).
    pub cache_dir: String,
    pub cache_ttl_secs: u64,
    pub max_bytes: usize,
    pub timeout_secs: u64,
    pub max_redirects: usize,
    /// Respektuj robots.txt (domyślnie tak).
    pub respect_robots: bool,
}

impl Default for FetchConfig {
    fn default() -> Self {
        let data_dir = default_data_dir();
        Self {
            cache_dir: data_dir.join("cache").to_string_lossy().to_string(),
            cache_ttl_secs: 3600,
            max_bytes: 2 * 1024 * 1024,
            timeout_secs: 20,
            max_redirects: 5,
            respect_robots: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    /// `none` | `seed-core` (feature `seed-gate`, faza 3).
    pub action_gate: String,
    /// Ingress proxy w trybie daemon (feature `server-gateway`).
    pub gateway: bool,
    pub audit_log: String,
    pub ssrf_strict: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            action_gate: "none".to_string(),
            gateway: false,
            audit_log: default_data_dir()
                .join("audit.jsonl")
                .to_string_lossy()
                .to_string(),
            ssrf_strict: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TransportConfig {
    pub tor: bool,
    pub tor_socks: String,
    pub onion_index: bool,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            tor: false,
            tor_socks: "127.0.0.1:9050".to_string(),
            onion_index: false,
        }
    }
}

/// Konfiguracja ekstrakcji twierdzeń.
///
/// `provider`: `ollama` | `openai` | `none`.
/// `fallback_provider`: `none` | `heuristic` | `ollama` | `openai`.
/// Klucz API jest najpierw czytany ze zmiennej środowiskowej wskazanej przez
/// `api_key_env` (albo `BALD_LLM_API_KEY`). Opcjonalny klucz w TOML jest
/// fallbackiem dla UI lokalnego i plik zawsze ma uprawnienia 0600.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    pub provider: String,
    pub fallback_provider: String,
    pub base_url: String,
    pub model: String,
    pub embedded_model: String,
    pub api_key_env: String,
    /// Lokalny fallback dla UI; env ma pierwszeństwo.
    pub api_key: Option<String>,
    pub fallback_base_url: String,
    pub fallback_model: String,
    pub fallback_api_key_env: String,
    /// Lokalny fallback dla UI; env ma pierwszeństwo.
    pub fallback_api_key: Option<String>,
    pub timeout_secs: u64,
    pub connect_timeout_secs: u64,
    pub temperature: f32,
    pub max_tokens: u32,
    pub max_input_chars: usize,
    pub max_claims: usize,
    pub max_attempts: usize,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            fallback_provider: "heuristic".to_string(),
            base_url: "http://127.0.0.1:11434".to_string(),
            model: "llama3.1:8b".to_string(),
            embedded_model: "qwen2.5-0.5b-instruct".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            api_key: None,
            fallback_base_url: "https://api.openai.com/v1".to_string(),
            fallback_model: "gpt-4o-mini".to_string(),
            fallback_api_key_env: "OPENAI_API_KEY".to_string(),
            fallback_api_key: None,
            timeout_secs: 60,
            connect_timeout_secs: 5,
            temperature: 0.1,
            max_tokens: 1200,
            max_input_chars: 14_000,
            max_claims: 8,
            max_attempts: 2,
        }
    }
}

impl LlmConfig {
    pub fn api_key(&self) -> Option<String> {
        read_secret("BALD_LLM_API_KEY")
            .or_else(|| read_secret(&self.api_key_env))
            .or_else(|| read_config_secret(self.api_key.as_deref()))
    }

    pub fn fallback_api_key(&self) -> Option<String> {
        read_secret("BALD_LLM_FALLBACK_API_KEY")
            .or_else(|| read_secret(&self.fallback_api_key_env))
            .or_else(|| read_config_secret(self.fallback_api_key.as_deref()))
    }
}

fn read_config_secret(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn read_secret(name: &str) -> Option<String> {
    if name.trim().is_empty() {
        return None;
    }
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProvidersConfig {
    pub searxng: SearxngConfig,
    pub brave: BraveConfig,
}

impl ProvidersConfig {
    pub fn any_enabled(&self) -> bool {
        self.searxng.enabled || self.brave.enabled
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SearxngConfig {
    pub enabled: bool,
    pub base_url: String,
    /// `None` = język z zapytania; `Some("pl")` = wymuś.
    pub language: Option<String>,
    /// 0 = off, 1 = moderate, 2 = strict.
    pub safesearch: u8,
}

impl Default for SearxngConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "http://127.0.0.1:8080".to_string(),
            language: None,
            safesearch: 0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BraveConfig {
    pub enabled: bool,
    pub api_key: Option<String>,
    /// np. `pl`; `None` = bez filtra kraju.
    pub country: Option<String>,
}

/// Konfiguracja autonomicznego szperacza modeli AI (Scout Bot) i trybu jazdy próbnej (A/B testing).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScoutConfig {
    pub enabled: bool,
    pub interval_days: u32,
    pub last_run: Option<String>,
    pub trial_model: Option<String>,
    pub previous_model: Option<String>,
    pub in_trial: bool,
}

impl Default for ScoutConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_days: 7,
            last_run: None,
            trial_model: None,
            previous_model: None,
            in_trial: false,
        }
    }
}

impl Config {
    /// Ładuje konfigurację z `BALD_CONFIG` albo domyślnej lokalizacji.
    /// Brak pliku = wartości domyślne (nie jest błędem).
    pub fn load() -> Result<Self, ConfigError> {
        let mut cfg = Self::load_file()?;
        cfg.apply_env();
        Ok(cfg)
    }

    /// Ładuje wyłącznie plik TOML, bez nadpisań ze środowiska.
    ///
    /// Ta ścieżka jest używana przez lokalne UI, żeby zapis ustawień nie
    /// utrwalał przypadkowo wartości pochodzących z env.
    pub fn load_file() -> Result<Self, ConfigError> {
        Self::load_from(&config_path())
    }

    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Config::default());
        }
        let raw = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&raw).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Zapisuje konfigurację do `BALD_CONFIG` albo domyślnej lokalizacji.
    pub fn save(&self) -> Result<(), ConfigError> {
        self.save_to(&config_path())
    }

    /// Atomowy zapis z uprawnieniami 0600 (Unix).
    pub fn save_to(&self, path: &Path) -> Result<(), ConfigError> {
        ensure_parent(path).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })?;
        let raw = toml::to_string_pretty(self).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source: std::io::Error::other(source),
        })?;
        let tmp = path.with_extension(format!("toml.tmp-{}", std::process::id()));
        write_private(&tmp, &raw).map_err(|source| ConfigError::Write {
            path: tmp.clone(),
            source,
        })?;
        std::fs::rename(&tmp, path).map_err(|source| {
            let _ = std::fs::remove_file(&tmp);
            ConfigError::Write {
                path: path.to_path_buf(),
                source,
            }
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    pub fn apply_env(&mut self) {
        if let Ok(v) = std::env::var("BALD_SEARXNG_ENABLED") {
            self.providers.searxng.enabled =
                parse_bool(&v).unwrap_or(self.providers.searxng.enabled);
        }
        if let Ok(v) = std::env::var("BALD_SEARXNG_URL") {
            if !v.trim().is_empty() {
                self.providers.searxng.base_url = v;
            }
        }
        if let Ok(v) = std::env::var("BALD_BRAVE_ENABLED") {
            self.providers.brave.enabled = parse_bool(&v).unwrap_or(self.providers.brave.enabled);
        }
        if let Ok(v) = std::env::var("BRAVE_API_KEY") {
            if !v.trim().is_empty() {
                self.providers.brave.api_key = Some(v);
            }
        }
        if let Ok(v) = std::env::var("BALD_AUDIT_LOG") {
            if !v.trim().is_empty() {
                self.security.audit_log = v;
            }
        }
        if let Ok(v) = std::env::var("BALD_STORE") {
            if !v.trim().is_empty() {
                self.storage.path = v;
            }
        }
        if let Ok(v) = std::env::var("BALD_INDEX_PATH") {
            if !v.trim().is_empty() {
                self.storage.index_path = v;
            }
        }
        if let Ok(v) = std::env::var("BALD_CACHE_DIR") {
            if !v.trim().is_empty() {
                self.fetch.cache_dir = v;
            }
        }
        if let Ok(v) = std::env::var("BALD_FETCH_TIMEOUT_SECS") {
            if let Ok(secs) = v.trim().parse::<u64>() {
                self.fetch.timeout_secs = secs;
            }
        }
        if let Ok(v) = std::env::var("BALD_RESPECT_ROBOTS") {
            self.fetch.respect_robots = parse_bool(&v).unwrap_or(self.fetch.respect_robots);
        }
        if let Ok(v) = std::env::var("BALD_TOR") {
            self.transport.tor = parse_bool(&v).unwrap_or(self.transport.tor);
        }
        if let Ok(v) = std::env::var("BALD_LLM_PROVIDER") {
            if !v.trim().is_empty() {
                self.llm.provider = v;
            }
        }
        if let Ok(v) = std::env::var("BALD_LLM_FALLBACK_PROVIDER") {
            if !v.trim().is_empty() {
                self.llm.fallback_provider = v;
            }
        }
        if let Ok(v) = std::env::var("BALD_LLM_BASE_URL") {
            if !v.trim().is_empty() {
                self.llm.base_url = v;
            }
        }
        if let Ok(v) = std::env::var("BALD_LLM_MODEL") {
            if !v.trim().is_empty() {
                self.llm.model = v;
            }
        }
        if let Ok(v) = std::env::var("BALD_LLM_API_KEY_ENV") {
            if !v.trim().is_empty() {
                self.llm.api_key_env = v;
            }
        }
        if let Ok(v) = std::env::var("BALD_LLM_FALLBACK_BASE_URL") {
            if !v.trim().is_empty() {
                self.llm.fallback_base_url = v;
            }
        }
        if let Ok(v) = std::env::var("BALD_LLM_FALLBACK_MODEL") {
            if !v.trim().is_empty() {
                self.llm.fallback_model = v;
            }
        }
        if let Ok(v) = std::env::var("BALD_LLM_FALLBACK_API_KEY_ENV") {
            if !v.trim().is_empty() {
                self.llm.fallback_api_key_env = v;
            }
        }
        if let Ok(v) = std::env::var("BALD_LLM_TIMEOUT_SECS") {
            if let Ok(secs) = v.trim().parse::<u64>() {
                self.llm.timeout_secs = secs;
            }
        }
        if let Ok(v) = std::env::var("BALD_LLM_CONNECT_TIMEOUT_SECS") {
            if let Ok(secs) = v.trim().parse::<u64>() {
                self.llm.connect_timeout_secs = secs;
            }
        }
        if let Ok(v) = std::env::var("BALD_LLM_TEMPERATURE") {
            if let Ok(value) = v.trim().parse::<f32>() {
                self.llm.temperature = value;
            }
        }
        if let Ok(v) = std::env::var("BALD_LLM_MAX_TOKENS") {
            if let Ok(value) = v.trim().parse::<u32>() {
                self.llm.max_tokens = value;
            }
        }
        if let Ok(v) = std::env::var("BALD_LLM_MAX_INPUT_CHARS") {
            if let Ok(value) = v.trim().parse::<usize>() {
                self.llm.max_input_chars = value;
            }
        }
        if let Ok(v) = std::env::var("BALD_LLM_MAX_CLAIMS") {
            if let Ok(value) = v.trim().parse::<usize>() {
                self.llm.max_claims = value;
            }
        }
        if let Ok(v) = std::env::var("BALD_LLM_MAX_ATTEMPTS") {
            if let Ok(value) = v.trim().parse::<usize>() {
                self.llm.max_attempts = value;
            }
        }
    }

    pub fn searxng_base_url(&self) -> &str {
        self.providers.searxng.base_url.trim_end_matches('/')
    }
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("BALD_CONFIG") {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    if let Some(dirs) = ProjectDirs::from("pl", "BALD", "bald") {
        return dirs.config_dir().join("config.toml");
    }
    PathBuf::from("bald.toml")
}

pub fn default_data_dir() -> PathBuf {
    if let Some(dirs) = ProjectDirs::from("pl", "BALD", "bald") {
        return dirs.data_dir().to_path_buf();
    }
    PathBuf::from(".bald")
}

/// Rozwija `~/...` na katalog domowy. Inne ścieżki pozostawia bez zmian.
pub fn expand_tilde(raw: &str) -> PathBuf {
    if raw == "~" {
        return home_dir().unwrap_or_else(|| PathBuf::from("~"));
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(raw)
}

fn home_dir() -> Option<PathBuf> {
    BaseDirs::new().map(|d| d.home_dir().to_path_buf())
}

pub fn ensure_parent(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

/// Zapisuje przykład konfiguracji z komentarzami. Nie nadpisuje bez `force`.
pub fn write_sample(path: &Path, force: bool) -> Result<bool, ConfigError> {
    if path.exists() && !force {
        return Ok(false);
    }
    ensure_parent(path).map_err(|source| ConfigError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    write_private(path, sample_toml()).map_err(|source| ConfigError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(true)
}

/// Zapisuje plik z uprawnieniami 0600 już przy tworzeniu (bez okna 0644→0600).
fn write_private(path: &Path, content: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, content)
    }
}

pub fn sample_toml() -> &'static str {
    r#"# BALD — konfiguracja lokalna (v0.1)
# Ten plik może zawierać klucze API. Uprawnienia 0600 są ustawiane przy `bald config init`.

[app]
profile = "local-first"      # local-first | hybrid | cloud
language = "pl"              # pl | en

[storage]
backend = "sqlite"           # sqlite (v1)
path = "~/.local/share/bald/bald.db"
index_path = "~/.local/share/bald/index"   # Tantivy

[fetch]
cache_dir = "~/.local/share/bald/cache"    # wyekstrahowane dokumenty
cache_ttl_secs = 3600
max_bytes = 2097152          # 2 MiB na odpowiedź
timeout_secs = 20
max_redirects = 5
respect_robots = true

[security]
action_gate = "none"         # none | seed-core (feature "seed-gate", faza 3)
gateway = false              # tylko tryb daemon (feature "server-gateway")
audit_log = "~/.local/share/bald/audit.jsonl"
ssrf_strict = true

[transport]
tor = false                  # v1.1, opt-in
tor_socks = "127.0.0.1:9050"
onion_index = false

[llm]
provider = "ollama"          # embedded | ollama | openai | none
fallback_provider = "heuristic"  # none | heuristic | ollama | openai
base_url = "http://127.0.0.1:11434"
model = "llama3.1:8b"
embedded_model = "qwen2.5-0.5b-instruct"  # qwen2.5-0.5b-instruct | qwen2.5-1.5b-instruct
api_key_env = "OLLAMA_API_KEY"   # env ma pierwszeństwo
# api_key = "..."               # opcjonalny fallback UI; plik ma 0600
fallback_base_url = "https://api.openai.com/v1"
fallback_model = "gpt-4o-mini"
fallback_api_key_env = "OPENAI_API_KEY"
# fallback_api_key = "..."
timeout_secs = 60
connect_timeout_secs = 5
temperature = 0.1
max_tokens = 1200
max_input_chars = 14000
max_claims = 8
max_attempts = 2

[providers.searxng]
enabled = false
base_url = "http://127.0.0.1:8080"
# language = "pl"
safesearch = 0               # 0 = off, 1 = moderate, 2 = strict

[providers.brave]
enabled = false
# api_key = "..."            # lepiej: export BRAVE_API_KEY=...
# country = "pl"
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_parses() {
        let cfg: Config = toml::from_str(sample_toml()).expect("sample must parse");
        assert_eq!(cfg.storage.backend, "sqlite");
        assert!(cfg.storage.index_path.ends_with("index"));
        assert_eq!(cfg.fetch.max_redirects, 5);
        assert_eq!(cfg.llm.provider, "ollama");
        assert_eq!(cfg.llm.fallback_provider, "heuristic");
        assert!(!cfg.providers.any_enabled());
    }

    #[test]
    fn tilde_expands() {
        let p = expand_tilde("~/x/y.db");
        assert!(!p.to_string_lossy().contains('~'));
        assert!(p.ends_with("x/y.db"));
    }

    #[test]
    fn save_and_load_without_env() {
        let dir = std::env::temp_dir().join(format!(
            "bald-config-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("config.toml");

        let mut cfg = Config::default();
        cfg.llm.api_key = Some("file-secret".to_string());
        cfg.providers.brave.api_key = Some("brave-secret".to_string());
        cfg.providers.searxng.enabled = true;
        cfg.save_to(&path).expect("save");

        let loaded = Config::load_from(&path).expect("load");
        assert!(loaded.providers.searxng.enabled);
        assert_eq!(loaded.llm.api_key.as_deref(), Some("file-secret"));
        assert_eq!(
            loaded.providers.brave.api_key.as_deref(),
            Some("brave-secret")
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
