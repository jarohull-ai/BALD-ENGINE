use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::hash::sha256_prefixed;
use crate::normalize::normalize_for_match;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationStatus {
    VerifiedExact,
    VerifiedNormalized,
    Rejected,
    SourceTooShort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationVerdict {
    pub status: CitationStatus,
    pub score: f32,
    pub quote_hash: String,
    pub source_hash: String,
    pub claim_hash: String,
    /// Zakres w **znormalizowanym** źródle (byte offsets), tylko dla exact.
    pub span: Option<Span>,
    pub matched_text: Option<String>,
}

impl CitationVerdict {
    pub fn is_verified(&self) -> bool {
        matches!(
            self.status,
            CitationStatus::VerifiedExact | CitationStatus::VerifiedNormalized
        )
    }
}

/// Deterministyczna walidacja cytatu. Nie wykrywa parafrazy semantycznej.
#[derive(Debug, Default, Clone, Copy)]
pub struct CitationGate;

impl CitationGate {
    pub fn new() -> Self {
        Self
    }

    pub fn verify(&self, claim: &str, quote: &str, source: &str) -> CitationVerdict {
        let claim_hash = sha256_prefixed(claim.trim());
        let quote_norm = normalize_for_match(quote);
        let source_norm = normalize_for_match(source);
        let quote_hash = sha256_prefixed(&quote_norm);
        let source_hash = sha256_prefixed(&source_norm);

        if quote_norm.chars().count() < 12
            || source_norm.chars().count() < quote_norm.chars().count()
        {
            return CitationVerdict {
                status: CitationStatus::SourceTooShort,
                score: 0.0,
                quote_hash,
                source_hash,
                claim_hash,
                span: None,
                matched_text: None,
            };
        }

        if let Some(start) = source_norm.find(&quote_norm) {
            return CitationVerdict {
                status: CitationStatus::VerifiedExact,
                score: 1.0,
                quote_hash,
                source_hash,
                claim_hash,
                span: Some(Span {
                    start,
                    end: start + quote_norm.len(),
                }),
                matched_text: Some(quote_norm),
            };
        }

        let quote_tokens: Vec<&str> = quote_norm.split_whitespace().collect();
        let source_tokens: Vec<&str> = source_norm.split_whitespace().collect();
        if quote_tokens.is_empty() || source_tokens.is_empty() {
            return CitationVerdict {
                status: CitationStatus::Rejected,
                score: 0.0,
                quote_hash,
                source_hash,
                claim_hash,
                span: None,
                matched_text: None,
            };
        }

        let q_set: HashSet<&str> = quote_tokens.iter().copied().collect();
        let mut best = 0.0f32;
        let windows: Vec<&[&str]> = if source_tokens.len() >= quote_tokens.len() {
            source_tokens.windows(quote_tokens.len()).collect()
        } else {
            vec![source_tokens.as_slice()]
        };

        for window in windows {
            let w_set: HashSet<&str> = window.iter().copied().collect();
            let intersection = q_set.intersection(&w_set).count() as f32;
            let union = (q_set.len() + w_set.len()) as f32 - intersection;
            if union > 0.0 {
                let jaccard = intersection / union;
                if jaccard > best {
                    best = jaccard;
                }
            }
        }

        let status = if best >= 0.92 {
            CitationStatus::VerifiedNormalized
        } else {
            CitationStatus::Rejected
        };

        CitationVerdict {
            status,
            score: best,
            quote_hash,
            source_hash,
            claim_hash,
            span: None,
            matched_text: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "Raport nie znalazł dowodów na to, że aktualizacje są „cichym balonem”. \
Autorzy podkreślają, że brak dowodów nie jest dowodem braku.";

    fn verify(quote: &str) -> CitationVerdict {
        CitationGate::new().verify("Aktualizacje są cichym balonem.", quote, SOURCE)
    }

    #[test]
    fn exact_after_normalization() {
        let v = verify("Raport nie znalazł dowodów na to, że aktualizacje są cichym balonem");
        assert_eq!(v.status, CitationStatus::VerifiedExact);
        assert_eq!(v.score, 1.0);
        assert!(v.span.is_some());
        assert!(v.quote_hash.starts_with("sha256:"));
    }

    #[test]
    fn punctuation_and_case_do_not_matter() {
        let v = verify("RAPORT NIE ZNALAZŁ DOWODÓW, NA TO, ŻE AKTUALIZACJE SĄ CICHYM BALONEM!");
        assert_eq!(v.status, CitationStatus::VerifiedExact);
    }

    #[test]
    fn paraphrase_is_rejected() {
        let v = verify("Naukowcy udowodnili, że aktualizacje to spisek rządu");
        assert_eq!(v.status, CitationStatus::Rejected);
        assert!(v.score < 0.92);
    }

    #[test]
    fn short_quote_is_rejected() {
        let v = verify("tak");
        assert_eq!(v.status, CitationStatus::SourceTooShort);
    }
}
