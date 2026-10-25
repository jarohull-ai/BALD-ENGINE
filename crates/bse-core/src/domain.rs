use serde::{Deserialize, Serialize};

/// Język zapytania. `Auto` jest rozwiązywany przez warstwę wyżej (CLI/UI/LLM).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lang {
    #[default]
    Auto,
    Pl,
    En,
}

impl Lang {
    pub fn code(self) -> Option<&'static str> {
        match self {
            Lang::Auto => None,
            Lang::Pl => Some("pl"),
            Lang::En => Some("en"),
        }
    }
}

/// Tier źródła. T1 = dokument pierwotny, T6 = treść nieznanego pochodzenia.
/// `Archive` jest celowo poza skalą — archiwum nie jest autorytetem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceTier {
    T1,
    T2,
    T3,
    T4,
    T5,
    T6,
    Archive,
}

/// Autorytet instytucjonalny — osobna oś od tieru. „Oficjalne” ≠ „prawdziwe”.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Authority {
    A,
    B,
    C,
    D,
}

/// Niezależność od innych źródeł w danym zestawie dowodów.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Independence {
    I0,
    I1,
    I2,
    I3,
}

/// Stosunek źródła do twierdzenia. Wymagany dla każdego dowodu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stance {
    Reports,
    Endorses,
    Debunks,
    Mentions,
}

/// Stan dostępności treści — fundament warstwy „ocenzurowane informacje”.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Live,
    Archived,
    Removed,
    Paywalled,
    Geoblocked,
    Unknown,
}
