//! BALD — typy domenowe współdzielone przez CLI, pipeline i (w przyszłości) UI.
//!
//! Ten crate nie zna providerów, HTTP, bazy ani LLM. Zawiera wyłącznie kontrakty.

pub mod domain;
pub mod error;
pub mod evidence;
pub mod gate;
pub mod provider;

pub use domain::{Authority, Availability, Independence, Lang, SourceTier, Stance};
pub use error::BseError;
pub use evidence::{
    ClaimKind, EntityKind, EvidenceBundle, EvidenceDocument, NewClaim, NewEntity, NewEvidence,
};
pub use gate::{Gate, GateCtx, GateDecision, GatePurpose, GateVerdict};
pub use provider::{
    Capabilities, Provider, ProviderError, ProviderStatus, RankedHit, RawHit, SearchQuery,
    SearchReport,
};
