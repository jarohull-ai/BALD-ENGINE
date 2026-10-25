//! Wspólna warstwa usług dla `bald serve` i komend Tauri.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use chrono::Utc;
use serde::Serialize;
use thiserror::Error;

use bse_config::{config_path, expand_tilde, Config, ConfigError};
use bse_store::{ClaimDetail, ClaimSummary, LedgerStats, Store, StoreError};

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

    #[error("service state lock poisoned")]
    Lock,
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
}
