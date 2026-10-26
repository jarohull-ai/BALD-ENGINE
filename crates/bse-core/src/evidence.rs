//! Typy dowodowe: twierdzenia, encje i wpisy do rejestru dowodowego.
//!
//! Ten moduł nie zna HTTP ani LLM. Opisuje wyłącznie to, co wolno zapisać
//! w ledgerze po przejściu przez `CitationGate`.

use serde::{Deserialize, Serialize};

use crate::domain::Stance;

/// Rodzaj twierdzenia. `Other` jest bezpiecznym fallbackiem, nie kategorią
/// „prawda/fałsz”.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    Fact,
    Causal,
    Concealment,
    Conspiracy,
    Prophecy,
    Other,
}

impl ClaimKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ClaimKind::Fact => "fact",
            ClaimKind::Causal => "causal",
            ClaimKind::Concealment => "concealment",
            ClaimKind::Conspiracy => "conspiracy",
            ClaimKind::Prophecy => "prophecy",
            ClaimKind::Other => "other",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "fact" | "factual" => Some(ClaimKind::Fact),
            "causal" | "cause" | "causality" => Some(ClaimKind::Causal),
            "concealment" | "hidden" | "cover_up" | "cover-up" => Some(ClaimKind::Concealment),
            "conspiracy" | "conspiratorial" => Some(ClaimKind::Conspiracy),
            "prophecy" | "prediction" => Some(ClaimKind::Prophecy),
            "other" | "unknown" => Some(ClaimKind::Other),
            _ => None,
        }
    }
}

/// Encje w ledgerze dowodowym. Obejmują organizacje, lokalizacje, dzieła oraz
/// postacie publiczne (aktorów narracji: polityków, ekspertów, autorów, instytucje).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    PublicActor,
    Organization,
    Location,
    Event,
    Document,
    Technology,
    Law,
    Work,
    Other,
}

impl EntityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EntityKind::PublicActor => "public_actor",
            EntityKind::Organization => "organization",
            EntityKind::Location => "location",
            EntityKind::Event => "event",
            EntityKind::Document => "document",
            EntityKind::Technology => "technology",
            EntityKind::Law => "law",
            EntityKind::Work => "work",
            EntityKind::Other => "other",
        }
    }

    /// Mapowanie odpowiedzi LLM.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "public_actor" | "public-actor" | "person" | "actor" | "politician" | "author"
            | "expert" | "leader" | "figure" | "official" | "individual" => {
                Some(EntityKind::PublicActor)
            }
            "organization" | "organisation" | "institution" | "org" | "company" => {
                Some(EntityKind::Organization)
            }
            "location" | "place" | "country" | "city" => Some(EntityKind::Location),
            "event" => Some(EntityKind::Event),
            "document" | "report" | "paper" => Some(EntityKind::Document),
            "technology" | "tech" | "software" => Some(EntityKind::Technology),
            "law" | "legislation" | "regulation" => Some(EntityKind::Law),
            "work" | "book" | "film" | "article" => Some(EntityKind::Work),
            "other" | "unknown" => Some(EntityKind::Other),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewEntity {
    pub id: String,
    pub kind: EntityKind,
    pub name: String,
    pub normalized: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewClaim {
    pub id: String,
    pub text: String,
    pub normalized: String,
    pub lang: Option<String>,
    pub kind: ClaimKind,
    pub entities: Vec<NewEntity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewEvidence {
    pub claim_id: String,
    pub stance: Stance,
    pub strength: f32,
    pub quote: String,
    pub quote_hash: String,
    pub span_start: Option<u32>,
    pub span_end: Option<u32>,
}

/// Dokument źródłowy, do którego odwołuje się evidence ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceDocument {
    pub canonical_url: String,
    pub title: String,
    pub domain: String,
    pub excerpt: String,
    pub content_hash: String,
    pub lang: Option<String>,
    pub published: Option<String>,
    pub fetched_at: String,
}

/// Jeden atomowy zapis: dokument + zaakceptowane twierdzenia + ich dowody.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceBundle {
    pub document: EvidenceDocument,
    pub claims: Vec<NewClaim>,
    pub evidence: Vec<NewEvidence>,
    pub model_version: String,
    pub prompt_version: String,
}
