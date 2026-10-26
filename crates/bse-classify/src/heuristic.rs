//! Deterministyczny fallback klasyfikatora.
//!
//! To nie jest „udawany LLM”: to regułowy ekstraktor zdań o wysokim sygnale,
//! używany offline i jako bezpieczny fallback, gdy lokalna Ollama nie odpowiada.

use async_trait::async_trait;
use bse_core::{ClaimKind, EntityKind, Stance};

use crate::filters::{contains_pii, normalize_claim_text, suspected_injection, truncate_chars};
use crate::{
    Classification, ClassifiedClaim, ClassifiedEntity, Classifier, ClassifyError, ClassifyInput,
    ClassifyOptions,
};

pub struct HeuristicClassifier {
    options: ClassifyOptions,
}

impl HeuristicClassifier {
    pub fn new(options: ClassifyOptions) -> Self {
        Self { options }
    }
}

impl Default for HeuristicClassifier {
    fn default() -> Self {
        Self::new(ClassifyOptions::default())
    }
}

#[async_trait]
impl Classifier for HeuristicClassifier {
    async fn classify(&self, input: &ClassifyInput) -> Result<Classification, ClassifyError> {
        if input.text.trim().is_empty() {
            return Err(ClassifyError::EmptyInput);
        }

        let total_chars = input.text.chars().count();
        let source_text = truncate_chars(&input.text, self.options.max_input_chars);
        let truncated = total_chars > self.options.max_input_chars;
        let injection = suspected_injection(&source_text);

        let mut candidates: Vec<(usize, i32, String)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (index, raw_sentence) in split_sentences(&source_text).into_iter().enumerate() {
            let sentence = raw_sentence.trim();
            let chars = sentence.chars().count();
            if !(24..=600).contains(&chars) {
                continue;
            }
            if contains_pii(sentence) {
                continue;
            }
            let normalized = normalize_claim_text(sentence);
            if normalized.is_empty() || !seen.insert(normalized) {
                continue;
            }
            let score = claim_score(sentence);
            if score <= 0 {
                continue;
            }
            candidates.push((index, score, sentence.to_string()));
        }

        candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        candidates.truncate(self.options.max_claims);

        let claims = candidates
            .into_iter()
            .map(|(_, score, sentence)| {
                let stance = heuristic_stance(&sentence);
                let kind = heuristic_kind(&sentence);
                let confidence = (0.45 + 0.05 * score as f32).min(0.85);
                let entities = known_entities(&sentence);
                ClassifiedClaim {
                    text: sentence.clone(),
                    quote: sentence,
                    stance,
                    kind,
                    confidence,
                    entities,
                }
            })
            .collect::<Vec<_>>();

        let mut warnings = vec!["heuristic_classifier".to_string()];
        if truncated {
            warnings.push("truncated_input".to_string());
        }
        if injection {
            warnings.push("prompt_injection_suspected".to_string());
        }
        if claims.is_empty() {
            warnings.push("no_claims_extracted".to_string());
        }

        Ok(Classification {
            claims,
            model: "heuristic-v1".to_string(),
            prompt_version: "heuristic-v1".to_string(),
            input_chars: total_chars,
            truncated,
            prompt_injection_suspected: injection,
            warnings,
        })
    }
}

fn split_sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut iter = text.char_indices().peekable();
    while let Some((index, ch)) = iter.next() {
        let boundary = match ch {
            '\n' | '!' | '?' => true,
            // Nie dziel `U.S.` ani `3.14`; zdanie kończy kropka przed spacją,
            // także gdy po kropce jest cudzysłów zamykający.
            '.' => {
                let mut look = iter.clone();
                let next_is_whitespace = loop {
                    match look.next() {
                        Some((_, '"' | '\'' | '”' | '’' | ')' | ']')) => {
                            continue;
                        }
                        Some((_, next)) => break next.is_whitespace(),
                        None => break true,
                    }
                };
                next_is_whitespace && !is_abbreviation(text, index)
            }
            _ => false,
        };
        if boundary {
            let end = index + ch.len_utf8();
            let piece = &text[start..end];
            if !piece.trim().is_empty() {
                out.push(piece);
            }
            start = end;
        }
    }
    if start < text.len() {
        let piece = &text[start..];
        if !piece.trim().is_empty() {
            out.push(piece);
        }
    }
    out
}

fn is_abbreviation(text: &str, period_index: usize) -> bool {
    let before = &text[..period_index];
    let token = before.split_whitespace().last().unwrap_or("");
    if token.is_empty() {
        return false;
    }
    let all_caps = token.chars().all(|ch| ch.is_ascii_uppercase() || ch == '.');
    let has_letter = token.chars().any(|ch| ch.is_ascii_alphabetic());
    if all_caps && has_letter && token.len() <= 6 {
        return true;
    }
    matches!(
        token.to_ascii_lowercase().as_str(),
        "dr" | "mr" | "mrs" | "ms" | "prof" | "np" | "etc" | "e.g" | "i.e" | "vs" | "al"
    )
}

fn claim_score(sentence: &str) -> i32 {
    let lower = sentence.to_lowercase();
    let words = sentence.split_whitespace().count();
    let mut score = if words >= 6 { 1 } else { 0 };
    const CUES: &[&str] = &[
        "ukryw",
        "tajn",
        "tajem",
        "spisek",
        "spisku",
        "elit",
        "kontrol",
        "plandemia",
        "plandemic",
        "depopulac",
        "globalist",
        "great reset",
        "nowy porz",
        "chemtrail",
        "mikroczip",
        "microchip",
        "szczepion",
        "vaccine",
        "fałsz",
        "falsz",
        "nieprawda",
        "mit",
        "debunk",
        "fact-check",
        "fact check",
        "no evidence",
        "nie ma dowod",
        "brak dowod",
        "cover-up",
        "coverup",
        "secretly",
        "rząd",
        "rzad",
        "wojsk",
        "military",
        "who",
        "nato",
        "ue ",
        "eu ",
    ];
    for cue in CUES {
        let matched = if matches!(*cue, "mit" | "who" | "nato" | "ue" | "eu") {
            contains_word(&lower, cue)
        } else {
            lower.contains(cue)
        };
        if matched {
            score += 2;
        }
    }
    if words < 10 {
        score -= 1;
    }
    score.max(0)
}

fn heuristic_stance(sentence: &str) -> Stance {
    let lower = sentence.to_lowercase();
    const DEBUNK: &[&str] = &[
        "fałsz",
        "falsz",
        "nieprawda",
        "mit",
        "debunk",
        "fact-check",
        "fact check",
        "nie ma dowod",
        "brak dowod",
        "no evidence",
        "false",
        "hoax",
        "obala",
        "dementuje",
        "refute",
        "refuted",
    ];
    const ENDORSE: &[&str] = &[
        "naprawdę",
        "naprawde",
        "ukryw",
        "tusz",
        "spisek",
        "spisku",
        "obudź",
        "obudz",
        "they are hiding",
        "cover-up",
        "coverup",
        "secretly",
        "globalist",
        "plandemia",
        "plandemic",
        "depopulac",
    ];
    const REPORT: &[&str] = &[
        "według",
        "wedlug",
        "twierdzi",
        "donosi",
        "raportuje",
        "reported",
        "claims",
        "alleged",
        "according to",
        "says",
    ];

    let debunks = DEBUNK.iter().any(|cue| {
        if *cue == "mit" {
            contains_word(&lower, cue)
        } else {
            lower.contains(cue)
        }
    });
    if debunks {
        Stance::Debunks
    } else if ENDORSE.iter().any(|cue| lower.contains(cue)) {
        Stance::Endorses
    } else if REPORT.iter().any(|cue| lower.contains(cue)) {
        Stance::Reports
    } else {
        Stance::Mentions
    }
}

fn heuristic_kind(sentence: &str) -> ClaimKind {
    let lower = sentence.to_lowercase();
    if ["ukryw", "tajn", "secret", "cover-up", "tusz"]
        .iter()
        .any(|cue| lower.contains(cue))
    {
        ClaimKind::Concealment
    } else if ["powoduje", "w wyniku", "caused", "because of", "due to"]
        .iter()
        .any(|cue| lower.contains(cue))
    {
        ClaimKind::Causal
    } else if ["spisek", "spisku", "conspiracy", "globalist", "elit"]
        .iter()
        .any(|cue| lower.contains(cue))
    {
        ClaimKind::Conspiracy
    } else if ["wkrótce", "wkrotce", "przysz", "soon", "will happen"]
        .iter()
        .any(|cue| lower.contains(cue))
    {
        ClaimKind::Prophecy
    } else {
        ClaimKind::Other
    }
}

const KNOWN_ENTITIES: &[(&str, EntityKind)] = &[
    ("NATO", EntityKind::Organization),
    ("WHO", EntityKind::Organization),
    ("World Health Organization", EntityKind::Organization),
    ("ONZ", EntityKind::Organization),
    ("United Nations", EntityKind::Organization),
    ("UN", EntityKind::Organization),
    ("European Union", EntityKind::Organization),
    ("Unia Europejska", EntityKind::Organization),
    ("Komisja Europejska", EntityKind::Organization),
    ("European Commission", EntityKind::Organization),
    ("World Economic Forum", EntityKind::Organization),
    ("WEF", EntityKind::Organization),
    ("FBI", EntityKind::Organization),
    ("CIA", EntityKind::Organization),
    ("CDC", EntityKind::Organization),
    ("Reuters", EntityKind::Organization),
    ("Associated Press", EntityKind::Organization),
    ("BBC", EntityKind::Organization),
    ("Google", EntityKind::Organization),
    ("Microsoft", EntityKind::Organization),
    ("Meta", EntityKind::Organization),
    ("Warsaw", EntityKind::Location),
    ("Warszawa", EntityKind::Location),
    ("Poland", EntityKind::Location),
    ("Polska", EntityKind::Location),
    ("Ukraine", EntityKind::Location),
    ("Ukraina", EntityKind::Location),
    ("Russia", EntityKind::Location),
    ("Rosja", EntityKind::Location),
    ("United States", EntityKind::Location),
    ("USA", EntityKind::Location),
    ("China", EntityKind::Location),
    ("Chiny", EntityKind::Location),
    ("Israel", EntityKind::Location),
    ("Izrael", EntityKind::Location),
    ("Gaza", EntityKind::Location),
    ("Brussels", EntityKind::Location),
    ("Bruksela", EntityKind::Location),
    ("COVID-19", EntityKind::Event),
    ("Great Reset", EntityKind::Event),
    ("Klaus Schwab", EntityKind::PublicActor),
    ("Edward Snowden", EntityKind::PublicActor),
    ("Rob Brotherton", EntityKind::PublicActor),
];

fn known_entities(text: &str) -> Vec<ClassifiedEntity> {
    let haystack = text.to_lowercase();
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (name, kind) in KNOWN_ENTITIES {
        let needle = name.to_lowercase();
        if !contains_bounded(&haystack, &needle) {
            continue;
        }
        let normalized = normalize_claim_text(name);
        if normalized.is_empty() || !seen.insert(normalized) {
            continue;
        }
        out.push(ClassifiedEntity {
            name: (*name).to_string(),
            kind: *kind,
        });
    }
    out
}

fn contains_word(haystack: &str, needle: &str) -> bool {
    contains_bounded(haystack, needle)
}

fn contains_bounded(haystack: &str, needle: &str) -> bool {
    let mut start = 0;
    while let Some(offset) = haystack[start..].find(needle) {
        let index = start + offset;
        let before = haystack[..index].chars().next_back();
        let after = haystack[index + needle.len()..].chars().next();
        let left_ok = before.map(|ch| !ch.is_alphanumeric()).unwrap_or(true);
        let right_ok = after.map(|ch| !ch.is_alphanumeric()).unwrap_or(true);
        if left_ok && right_ok {
            return true;
        }
        start = index + needle.len();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(text: &str) -> ClassifyInput {
        ClassifyInput::new(
            "Title",
            "https://example.org/a",
            text,
            Some("en".to_string()),
        )
    }

    #[tokio::test]
    async fn extracts_a_claim_with_a_verbatim_quote() {
        let text = "Fact-checkers say the vaccine microchip claim is false and has no evidence.";
        let result = HeuristicClassifier::default()
            .classify(&input(text))
            .await
            .unwrap();
        assert_eq!(result.claims.len(), 1);
        assert_eq!(result.claims[0].quote, text);
        assert_eq!(result.claims[0].stance, Stance::Debunks);
        assert_eq!(result.claims[0].kind, ClaimKind::Other);
        assert!(result
            .warnings
            .contains(&"heuristic_classifier".to_string()));
    }

    #[tokio::test]
    async fn marks_endorsement_and_nonperson_entity() {
        let text = "NATO secretly controls the weather and the government hides it from us.";
        let result = HeuristicClassifier::default()
            .classify(&input(text))
            .await
            .unwrap();
        assert_eq!(result.claims.len(), 1);
        assert_eq!(result.claims[0].stance, Stance::Endorses);
        assert_eq!(result.claims[0].entities[0].name, "NATO");
        assert_eq!(result.claims[0].entities[0].kind, EntityKind::Organization);
    }

    #[tokio::test]
    async fn skips_pii_and_short_text() {
        let text = "Kontakt: jan@example.com lub telefon 123 456 789. Krótko.";
        let result = HeuristicClassifier::default()
            .classify(&input(text))
            .await
            .unwrap();
        assert!(result.claims.is_empty());
    }

    #[test]
    fn short_words_need_boundaries_and_topic_names_are_not_endorsements() {
        let summit = "The summit was postponed and the Great Reset was discussed.";
        assert_eq!(heuristic_stance(summit), Stance::Mentions);
        assert_eq!(claim_score("The summit was postponed."), 0);
    }

    #[test]
    fn does_not_split_abbreviations_or_decimals() {
        let pieces =
            split_sentences("The U.S. policy changed in 2024. Inflation was 3.14 percent.");
        assert_eq!(pieces.len(), 2);
        assert!(pieces[0].starts_with("The U.S. policy"));
    }

    #[test]
    fn splits_after_period_before_closing_quote() {
        let pieces = split_sentences("A sentence ends.\" Next starts.");
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0], "A sentence ends.");
    }
}
