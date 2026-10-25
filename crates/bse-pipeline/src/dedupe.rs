//! Near-duplicate suppression dla wyników wyszukiwania.
//!
//! Ten sam materiał często wraca z trzech providerów pod różnymi URL-ami
//! (AMP, mirror, parametry). Kanoniczny URL tego nie złapie, więc liczymy
//! MinHash na shinglach z `title + snippet` i grupujemy przez LSH.
//!
//! Parametry są celowo konserwatywne: 4-tokenowe shingle, 64 hashe, 16 pasm
//! po 4 wartości i próg Jaccarda 0.90. Krótkie snippety (< 8 tokenów) nie są
//! deduplikowane, żeby nie scalać przypadkowych wyników.

use std::collections::{HashMap, HashSet};

use bse_core::RankedHit;

const SHINGLE_TOKENS: usize = 4;
const MIN_TOKENS: usize = 8;
const HASH_COUNT: usize = 64;
const BAND_COUNT: usize = 16;
const BAND_ROWS: usize = HASH_COUNT / BAND_COUNT;
const THRESHOLD: f32 = 0.90;

pub(crate) fn near_dedupe(hits: &mut Vec<RankedHit>) -> usize {
    let mut kept: Vec<RankedHit> = Vec::with_capacity(hits.len());
    let mut signatures: Vec<Option<Signature>> = Vec::with_capacity(hits.len());
    let mut lsh = Lsh::default();
    let mut removed = 0usize;

    for hit in hits.drain(..) {
        // Dłuższy snippet jest stabilniejszy niż tytuł: mirrory często mają
        // inny tytuł, ale identyczny lub prawie identyczny opis.
        let source = if tokenize(&hit.snippet).len() >= MIN_TOKENS {
            hit.snippet.clone()
        } else {
            format!("{} {}", hit.title, hit.snippet)
        };
        let signature = Signature::from_text(&source);

        let duplicate_of = signature.as_ref().and_then(|candidate| {
            let mut seen = HashSet::new();
            for index in lsh.candidates(candidate) {
                if !seen.insert(index) {
                    continue;
                }
                let Some(existing) = signatures.get(index).and_then(Option::as_ref) else {
                    continue;
                };
                if candidate.similarity(existing) >= THRESHOLD {
                    return Some(index);
                }
            }
            None
        });

        if let Some(index) = duplicate_of {
            let existing = &mut kept[index];
            for provider in &hit.providers {
                if !existing.providers.contains(provider) {
                    existing.providers.push(provider.clone());
                }
            }
            existing.best_rank = existing.best_rank.min(hit.best_rank);
            if existing.title.is_empty() && !hit.title.is_empty() {
                existing.title = hit.title;
            }
            if existing.snippet.chars().count() < hit.snippet.chars().count() {
                existing.snippet = hit.snippet;
            }
            removed += 1;
            continue;
        }

        if let Some(signature) = &signature {
            lsh.insert(signature, kept.len());
        }
        signatures.push(signature);
        kept.push(hit);
    }

    *hits = kept;
    removed
}

#[derive(Clone)]
struct Signature {
    values: [u64; HASH_COUNT],
}

impl Signature {
    fn from_text(text: &str) -> Option<Self> {
        let tokens = tokenize(text);
        if tokens.len() < MIN_TOKENS {
            return None;
        }

        let shingles: Vec<String> = tokens
            .windows(SHINGLE_TOKENS)
            .map(|window| window.join(" "))
            .collect();
        if shingles.is_empty() {
            return None;
        }

        let mut values = [u64::MAX; HASH_COUNT];
        for shingle in shingles {
            let base = fnv1a(shingle.as_bytes());
            for (index, slot) in values.iter_mut().enumerate() {
                *slot = (*slot).min(mix64(base ^ seed(index)));
            }
        }
        Some(Self { values })
    }

    fn similarity(&self, other: &Self) -> f32 {
        let matching = self
            .values
            .iter()
            .zip(other.values.iter())
            .filter(|(a, b)| a == b)
            .count();
        matching as f32 / HASH_COUNT as f32
    }

    fn band_key(&self, band: usize) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let start = band * BAND_ROWS;
        for value in &self.values[start..start + BAND_ROWS] {
            hash ^= *value;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        mix64(hash)
    }
}

#[derive(Default)]
struct Lsh {
    bands: Vec<HashMap<u64, Vec<usize>>>,
}

impl Lsh {
    fn candidates(&self, signature: &Signature) -> Vec<usize> {
        let mut out = Vec::new();
        for band in 0..BAND_COUNT {
            if let Some(bucket) = self.bands.get(band) {
                if let Some(indices) = bucket.get(&signature.band_key(band)) {
                    out.extend(indices.iter().copied());
                }
            }
        }
        out
    }

    fn insert(&mut self, signature: &Signature, index: usize) {
        if self.bands.is_empty() {
            self.bands = (0..BAND_COUNT).map(|_| HashMap::new()).collect();
        }
        for band in 0..BAND_COUNT {
            self.bands[band]
                .entry(signature.band_key(band))
                .or_default()
                .push(index);
        }
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|token| token.chars().count() >= 2)
        .map(|token| token.to_lowercase())
        .collect()
}

fn seed(index: usize) -> u64 {
    mix64(0x9e37_79b9_7f4a_7c15 ^ index as u64)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(url: &str, title: &str, snippet: &str, provider: &str) -> RankedHit {
        RankedHit {
            title: title.into(),
            url: url.into(),
            canonical_url: url.into(),
            snippet: snippet.into(),
            score: 1.0,
            providers: vec![provider.into()],
            best_rank: 1,
            published: None,
        }
    }

    #[test]
    fn merges_near_identical_mirrors() {
        let text = "Cichy balon nad miastem to narracja o rzekomym obiekcie obserwowanym nocą przez wielu świadków";
        let mut hits = vec![
            hit("https://a.example/story", "Cichy balon", text, "searxng"),
            hit("https://b.example/mirror", "Cichy balon", text, "brave"),
            hit(
                "https://c.example/inny",
                "Raport o jakości wody",
                "Zupełnie inny dokument o wodzie, filtrach i wynikach badań laboratoryjnych",
                "demo",
            ),
        ];
        assert_eq!(near_dedupe(&mut hits), 1);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].providers, vec!["searxng", "brave"]);
    }

    #[test]
    fn does_not_merge_short_or_distinct_snippets() {
        let mut hits = vec![
            hit("https://a.example/1", "A", "snippet A", "demo"),
            hit("https://b.example/2", "B", "snippet B", "demo"),
            hit(
                "https://c.example/3",
                "Raport wodny",
                "Treść o jakości wody, filtrach i badaniach laboratoryjnych w mieście",
                "demo",
            ),
            hit(
                "https://d.example/4",
                "Raport balonowy",
                "Treść o obserwacjach obiektu na niebie i relacjach świadków z miasta",
                "demo",
            ),
        ];
        assert_eq!(near_dedupe(&mut hits), 0);
        assert_eq!(hits.len(), 4);
    }

    #[test]
    fn same_text_with_different_titles_is_merged() {
        let body = "Ten sam materiał w trzech miejscach opisuje rzekomy cichy balon i reakcję lokalnych mediów";
        let mut hits = vec![
            hit("https://a.example/x", "Cichy balon", body, "searxng"),
            hit("https://b.example/y", "Cichy balon — mirror", body, "brave"),
        ];
        assert_eq!(near_dedupe(&mut hits), 1);
    }
}
