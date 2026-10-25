//! Wspólne API lokalne BALD dla web UI (`bald serve`) i Tauri IPC.
//!
//! Warstwa `Service` nie zna HTTP ani Tauri. `server` dokłada loopbackowy
//! serwer z tokenem sesji, a `apps/desktop` mapuje te same metody na komendy
//! Tauri. Dzięki temu UI ma jeden kontrakt danych w obu trybach.

mod server;
mod service;
mod settings;

pub use server::{serve, ServeOptions};
pub use service::{ClaimsResponse, Service, ServiceError};
pub use settings::{
    apply_update, view, BraveView, LlmView, SearxngView, SettingsError, SettingsUpdate,
    SettingsView,
};

pub use bse_store::{ClaimDetail, ClaimSummary, EntityItem, EvidenceItem, LedgerStats};
