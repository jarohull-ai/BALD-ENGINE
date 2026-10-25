//! Bramki BALD: deterministyczny person guard, walidacja cytowań i audit trail.
//!
//! Zasady:
//! - `Gate::check` jest synchroniczny, bez I/O i bez sieci.
//! - Błąd wewnętrzny bramki = `Refuse` (fail-closed).
//! - Audit nie przechowuje surowych zapytań ani PII — tylko hashe i kody reguł.

pub mod audit;
pub mod citation;
pub mod hash;
pub mod normalize;
pub mod rules;

pub use audit::{subject_query, subject_text, AuditChain, AuditError, AuditLog, AuditRecord};
pub use citation::{CitationGate, CitationStatus, CitationVerdict, Span};
pub use hash::{sha256_hex, sha256_prefixed};
pub use normalize::{normalize_for_match, normalize_for_rules};
pub use rules::RulesGate;
