//! Silnik oceny wiarygodności źródeł i odpowiedzi (Evidence & Credibility Judge).
//!
//! Oblicza transparentny, matematyczny wskaźnik wiarygodności (0-100%)
//! na podstawie autorytetu domeny (T1-T5), gęstości faktograficznej (daty, liczby, cytaty),
//! niezależnej korelacji między źródłami oraz zgodności z Rejestrem Dowodowym BALD.

use serde::{Deserialize, Serialize};

use crate::service::{ChatSource, SearchHit};
use bse_store::ClaimSummary;

/// Ocena wiarygodności pojedynczego źródła lub wyniku wyszukiwania.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredibilityScore {
    pub score: u32,
    pub tier: String,
    pub tier_label: String,
    pub badge: String,
    pub badge_color: String,
    pub domain_authority: u32,
    pub factual_density: u32,
    pub corroboration_count: usize,
    pub reason: String,
    pub factors: Vec<String>,
}

/// Zbiorcza ocena wiarygodności i weryfikacji całej odpowiedzi asystenta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerCredibility {
    pub overall_score: u32,
    pub verdict: String,
    pub verdict_detail: String,
    pub badge_color: String,
    pub sources_evaluated: usize,
    pub primary_sources_count: usize,
    pub corroboration_level: String,
    pub key_strengths: Vec<String>,
}

/// Ocenia pojedynczy wynik wyszukiwania w kontekście całego zestawu wyników.
pub fn evaluate_hit(
    hit: &SearchHit,
    all_hits: &[SearchHit],
    matching_claims: &[ClaimSummary],
) -> CredibilityScore {
    let domain = hit.domain.to_ascii_lowercase();
    let (domain_authority, tier, tier_label) = classify_domain(&domain);

    let factual_density = calculate_factual_density(&hit.title, &hit.snippet);

    let corroboration_count = calculate_corroboration(&hit.title, all_hits);

    let ledger_match = matching_claims.iter().any(|c| {
        c.text.split_whitespace().any(|word| {
            word.len() > 4
                && (hit.title.to_lowercase().contains(&word.to_lowercase())
                    || hit.snippet.to_lowercase().contains(&word.to_lowercase()))
        })
    });

    let mut factors = Vec::new();
    let mut raw_score = (domain_authority as f32 * 0.45)
        + (factual_density as f32 * 0.30)
        + ((corroboration_count.min(3) as f32 / 3.0) * 15.0);

    if hit.providers.len() > 1 {
        raw_score += 5.0;
        factors.push("Potwierdzone przez wielu niezależnych dostawców wyszukiwania".to_string());
    }

    if ledger_match {
        raw_score += 10.0;
        factors.push("Zgodność ze zweryfikowanym twierdzeniem w Rejestrze BALD".to_string());
    }

    if hit.published.is_some() {
        factors.push("Zweryfikowana data publikacji".to_string());
    }

    if factual_density >= 60 {
        factors.push("Wysoka gęstość faktograficzna (konkretne dane/daty)".to_string());
    }

    factors.push(format!("{tier_label} (autorytet {domain_authority}%)"));

    let score = (raw_score as u32).clamp(15, 98);

    let (badge, badge_color) = if score >= 80 {
        ("Wysoka wiarygodność".to_string(), "emerald".to_string())
    } else if score >= 55 {
        ("Umiarkowana wiarygodność".to_string(), "amber".to_string())
    } else {
        (
            "Wymaga weryfikacji".to_string(),
            "rose".to_string(),
        )
    };

    let reason = format!(
        "{tier_label}. Indeks korelacji: {} innych źródeł popiera te słowa kluczowe.",
        corroboration_count
    );

    CredibilityScore {
        score,
        tier,
        tier_label,
        badge,
        badge_color,
        domain_authority,
        factual_density,
        corroboration_count,
        reason,
        factors,
    }
}

/// Ocenia syntetyzowaną odpowiedź asystenta na podstawie cytowanych źródeł i twierdzeń.
pub fn evaluate_answer(
    content: &str,
    sources: &[ChatSource],
    matching_claims: &[ClaimSummary],
) -> AnswerCredibility {
    let sources_evaluated = sources.len();
    if sources_evaluated == 0 && matching_claims.is_empty() {
        return AnswerCredibility {
            overall_score: 50,
            verdict: "Odpowiedź bazowa".to_string(),
            verdict_detail: "Brak zewnętrznych źródeł online lub twierdzeń w rejestrze — odpowiedź oparta o wiedzę wstępną modelu."
                .to_string(),
            badge_color: "amber".to_string(),
            sources_evaluated: 0,
            primary_sources_count: 0,
            corroboration_level: "Brak źródeł zewnętrznych".to_string(),
            key_strengths: vec!["Generacja konwersacyjna".to_string()],
        };
    }

    let mut primary_sources_count = 0;
    let mut total_domain_auth = 0;

    for s in sources {
        let (auth, tier, _) = classify_domain(&s.domain);
        total_domain_auth += auth;
        if tier == "T1" || tier == "T2" {
            primary_sources_count += 1;
        }
    }

    let avg_domain_auth = if sources_evaluated > 0 {
        total_domain_auth / sources_evaluated as u32
    } else {
        70
    };

    let mut key_strengths = Vec::new();

    // Wykrywanie cytowań [1], [2] itp. w treści odpowiedzi
    let mut citation_count = 0;
    for i in 1..=sources.len().max(1) {
        let bracket = format!("[{i}]");
        if content.contains(&bracket) {
            citation_count += 1;
        }
    }

    let mut score = (avg_domain_auth as f32 * 0.40)
        + ((sources_evaluated.min(5) as f32 / 5.0) * 25.0)
        + ((citation_count.min(sources_evaluated) as f32
            / sources_evaluated.max(1) as f32)
            * 20.0);

    if !matching_claims.is_empty() {
        score += 15.0;
        key_strengths.push(format!(
            "Skorelowano z {} twierdzeniami z Rejestru Dowodowego",
            matching_claims.len()
        ));
    }

    if primary_sources_count > 0 {
        key_strengths.push(format!(
            "Obecność {} źródeł pierwotnych / referencyjnych (T1/T2)",
            primary_sources_count
        ));
    }

    if citation_count > 0 {
        key_strengths.push(format!(
            "Bezpośrednie cytowania odnośnikowe ({citation_count}/{sources_evaluated} źródeł)",
        ));
    }

    let overall_score = (score as u32).clamp(25, 99);

    let (verdict, verdict_detail, badge_color) = if overall_score >= 80 {
        (
            "Wysoka wiarygodność".to_string(),
            format!(
                "Fakty poparte przez {sources_evaluated} niezależnych źródeł, w tym {primary_sources_count} źródła T1/T2 z bezpośrednimi odnośnikami."
            ),
            "emerald".to_string(),
        )
    } else if overall_score >= 60 {
        (
            "Umiarkowana wiarygodność".to_string(),
            format!(
                "Odpowiedź poparta {sources_evaluated} źródłami internetowymi. Zalecana weryfikacja kluczowych dat i liczb."
            ),
            "amber".to_string(),
        )
    } else {
        (
            "Wymaga weryfikacji".to_string(),
            "Ograniczona liczba niezależnych źródeł lub brak bezpośrednich cytowań w rejestrze."
                .to_string(),
            "rose".to_string(),
        )
    };

    let corroboration_level = if sources_evaluated >= 3 && primary_sources_count >= 1 {
        "Wielostronna korelacja dowodowa".to_string()
    } else if sources_evaluated >= 2 {
        "Dwuźródłowe potwierdzenie".to_string()
    } else {
        "Pojedyncze źródło informacji".to_string()
    };

    AnswerCredibility {
        overall_score,
        verdict,
        verdict_detail,
        badge_color,
        sources_evaluated,
        primary_sources_count,
        corroboration_level,
        key_strengths,
    }
}

/// Klasyfikuje domenę pod kątem Tieru wiarygodności (T1..T5) i autorytetu.
fn classify_domain(domain: &str) -> (u32, String, String) {
    let d = domain.trim().trim_start_matches("www.").to_ascii_lowercase();

    // T1: Instytucje rządowe, unijne, edukacyjne, akademickie, urzędowe
    if d.ends_with(".gov.pl")
        || d.ends_with(".gov")
        || d.ends_with(".europa.eu")
        || d.ends_with(".sejm.gov.pl")
        || d.ends_with(".senat.gov.pl")
        || d.ends_with(".edu.pl")
        || d.ends_with(".edu")
        || d.ends_with(".ac.uk")
        || d.ends_with(".mil")
        || d == "nbp.pl"
        || d == "stat.gov.pl"
        || d == "pan.pl"
        || d == "who.int"
        || d == "un.org"
        || d == "arxiv.org"
    {
        return (
            95,
            "T1".to_string(),
            "Źródło pierwotne / oficjalne (T1)".to_string(),
        );
    }

    // T2: Bazy wiedzy, encyklopedie, recenzowane repozytoria, repozytoria kodu i dokumentacji
    if d.contains("wikipedia.org")
        || d == "nature.com"
        || d == "science.org"
        || d == "reuters.com"
        || d == "apnews.com"
        || d == "pap.pl"
        || d == "github.com"
        || d == "docs.rs"
        || d == "developer.mozilla.org"
        || d == "ietf.org"
        || d == "w3.org"
    {
        return (
            85,
            "T2".to_string(),
            "Uznane źródło referencyjne (T2)".to_string(),
        );
    }

    // T3: Główne portale informacyjne, analityczne i branżowe
    if d.ends_with(".org")
        || d == "bbc.com"
        || d == "theguardian.com"
        || d == "wyborcza.pl"
        || d == "rp.pl"
        || d == "onet.pl"
        || d == "wp.pl"
        || d == "interia.pl"
        || d == "money.pl"
        || d == "bankier.pl"
        || d == "forsal.pl"
        || d == "spidersweb.pl"
        || d == "benchmark.pl"
    {
        return (
            70,
            "T3".to_string(),
            "Medium prasowe / analityczne (T3)".to_string(),
        );
    }

    // T5: Fora, platformy społecznościowe i treści tworzone przez użytkowników
    if d.contains("reddit.com")
        || d.contains("twitter.com")
        || d.contains("x.com")
        || d.contains("facebook.com")
        || d.contains("medium.com")
        || d.contains("blogspot.com")
        || d.contains("wordpress.com")
    {
        return (
            35,
            "T5".to_string(),
            "Platforma społecznościowa / forum (T5)".to_string(),
        );
    }

    // T4: Pozostałe domeny komercyjne / internetowe
    (
        55,
        "T4".to_string(),
        "Portal internetowy ogólny (T4)".to_string(),
    )
}

/// Oblicza gęstość faktograficzną (obecność lat, dat, liczb, cytowań).
fn calculate_factual_density(title: &str, snippet: &str) -> u32 {
    let combined = format!("{title} {snippet}");
    let mut score = 30u32;

    // Obecność roku (19xx lub 20xx)
    if combined
        .split(|c: char| !c.is_numeric())
        .any(|part| part.len() == 4 && (part.starts_with("19") || part.starts_with("20")))
    {
        score += 20;
    }

    // Obecność liczb / jednostek / procentów
    if combined.contains('%')
        || combined.contains("zł")
        || combined.contains("PLN")
        || combined.contains('$')
        || combined.contains("EUR")
        || combined.split_whitespace().any(|w| w.chars().any(|c| c.is_numeric()))
    {
        score += 20;
    }

    // Cytowania cudzysłowowe
    if combined.contains('"') || combined.contains('„') || combined.contains('”') {
        score += 15;
    }

    // Długość i struktura wypowiedzi
    if snippet.len() > 140 {
        score += 15;
    }

    score.min(100)
}

/// Sprawdza ile innych wyników zawiera wspólne kluczowe tokeny (korelacja krzyżowa).
fn calculate_corroboration(title: &str, all_hits: &[SearchHit]) -> usize {
    let keywords: Vec<String> = title
        .split_whitespace()
        .filter(|w| w.len() > 4)
        .map(|w| w.to_lowercase())
        .collect();

    if keywords.is_empty() {
        return 0;
    }

    let mut count = 0;
    for other in all_hits {
        if other.title == title {
            continue;
        }
        let other_lower = format!("{} {}", other.title, other.snippet).to_lowercase();
        let matches = keywords.iter().filter(|k| other_lower.contains(*k)).count();
        if matches >= 2 {
            count += 1;
        }
    }

    count
}
