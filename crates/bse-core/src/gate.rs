use serde::{Deserialize, Serialize};

use crate::domain::Lang;

/// Po co uruchamiamy bramkę. `Action` dotyczy wyłącznie akcji własnych BALD
/// (fetch, eksport, otwarcie linku), nie zapytań użytkownika.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatePurpose {
    Search,
    Fetch,
    Publish,
    Export,
    Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateDecision {
    Allow,
    Rewrite,
    Review,
    Refuse,
}

impl GateDecision {
    /// Im wyżej, tym bardziej restrykcyjna decyzja.
    pub fn severity(self) -> u8 {
        match self {
            GateDecision::Allow => 0,
            GateDecision::Rewrite => 1,
            GateDecision::Review => 2,
            GateDecision::Refuse => 3,
        }
    }

    pub fn is_refused(self) -> bool {
        matches!(self, GateDecision::Refuse)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateVerdict {
    pub gate: String,
    pub decision: GateDecision,
    /// 0.0..=1.0 — tylko do UI/audytu, nie jest „prawdopodobieństwem prawdy”.
    pub score: f32,
    /// Kody powodów, nigdy surowe PII.
    pub reasons: Vec<String>,
    /// Identyfikatory reguł / kategorie.
    pub matched: Vec<String>,
    /// Propozycja bezpieczniejszego zapytania (tylko `Rewrite`).
    pub rewritten: Option<String>,
}

impl GateVerdict {
    pub fn allow(gate: impl Into<String>) -> Self {
        Self {
            gate: gate.into(),
            decision: GateDecision::Allow,
            score: 0.0,
            reasons: Vec::new(),
            matched: Vec::new(),
            rewritten: None,
        }
    }

    pub fn refuse(
        gate: impl Into<String>,
        reason: impl Into<String>,
        matched: Vec<String>,
    ) -> Self {
        Self {
            gate: gate.into(),
            decision: GateDecision::Refuse,
            score: 1.0,
            reasons: vec![reason.into()],
            matched,
            rewritten: None,
        }
    }

    pub fn review(
        gate: impl Into<String>,
        reason: impl Into<String>,
        matched: Vec<String>,
    ) -> Self {
        Self {
            gate: gate.into(),
            decision: GateDecision::Review,
            score: 0.5,
            reasons: vec![reason.into()],
            matched,
            rewritten: None,
        }
    }

    pub fn is_refused(&self) -> bool {
        self.decision.is_refused()
    }
}

pub struct GateCtx<'a> {
    pub text: &'a str,
    pub lang: Lang,
    pub purpose: GatePurpose,
    /// Dla `SeedGate`: `GeometryMetrics.etm`.
    pub citation_coverage: Option<f32>,
    /// Dla `SeedGate`: `GeometryMetrics.lid`.
    pub semantic_dispersion: Option<f32>,
}

impl<'a> GateCtx<'a> {
    pub fn search(text: &'a str, lang: Lang) -> Self {
        Self {
            text,
            lang,
            purpose: GatePurpose::Search,
            citation_coverage: None,
            semantic_dispersion: None,
        }
    }
}

/// Bramka jest synchroniczna, bez I/O i bez sieci. Błąd wewnętrzny = `Refuse`.
pub trait Gate: Send + Sync {
    fn id(&self) -> &'static str;
    fn check(&self, ctx: &GateCtx<'_>) -> GateVerdict;
}
