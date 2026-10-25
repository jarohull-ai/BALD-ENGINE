use async_trait::async_trait;
use bse_core::{Capabilities, Provider, ProviderError, RawHit, SearchQuery};

/// Offline provider do developmentu i testów UI bez sieci i bez kluczy.
/// Treści są fikcyjne — służą tylko do pokazania kształtu wyników.
pub struct DemoProvider;

impl DemoProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DemoProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for DemoProvider {
    fn id(&self) -> &str {
        "demo"
    }

    fn display_name(&self) -> &str {
        "Demo (offline fixtures)"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            web: true,
            news: true,
            archive: true,
            factcheck: true,
            render: false,
        }
    }

    async fn search(&self, _query: &SearchQuery) -> Result<Vec<RawHit>, ProviderError> {
        let items = [
            (
                "Cichy Balon: czy aktualizacje naprawdę są ukrywane?",
                "https://example.org/demo/cichy-balon",
                "Blog opisuje rzekomy „cichy balon” — mechanizm ukrywania zmian w oprogramowaniu. Brak dokumentów źródłowych; powołuje się na anonimowe wpisy.",
                "2024-11-03",
            ),
            (
                "Fact-check: „Cichy Balon” nie istnieje",
                "https://example.org/demo/fact-check-cichy-balon",
                "Redakcja sprawdziła 14 wersji narracji i nie znalazła dowodów na celowe ukrywanie aktualizacji. Wskazuje na mylenie pojęć: supply chain vs. telemetria.",
                "2025-02-14",
            ),
            (
                "Archiwum: usunięty wątek o „Cichym Balonie”",
                "https://example.org/demo/archive-thread",
                "Snapshot wątku z forum z 2023 r. Wątek został usunięty przez moderatora; dostępny w Wayback Machine. Zawiera 42 komentarze i odwołania do nieistniejącego raportu.",
                "2023-08-21",
            ),
            (
                "Transparency of software supply chains — przegląd badań",
                "https://example.org/demo/supply-chain-review",
                "Przegląd 31 badań o przejrzystości łańcucha dostaw oprogramowania. Omawia telemetrię, reproducible builds i ograniczenia audytu.",
                "2025-06-01",
            ),
            (
                "Deplatforming i obieg narracji w Europie Środkowej",
                "https://example.org/demo/deplatforming",
                "Analiza przypadków usuwania treści i migracji społeczności. Autorzy rozróżniają cenzurę prawną, moderację platform i samo-cenzurę.",
                "2024-12-09",
            ),
            (
                "Snapshot: strona projektu „Cichy Balon” (2024)",
                "https://example.org/demo/snapshot-2024",
                "Zarchiwizowana kopia strony projektu. Strona zniknęła w styczniu 2025; snapshot pokazuje deklarację „brak zaufania do dostawców”.",
                "2024-09-18",
            ),
        ];

        Ok(items
            .into_iter()
            .enumerate()
            .map(|(idx, (title, url, snippet, published))| RawHit {
                title: title.to_string(),
                url: url.to_string(),
                snippet: snippet.to_string(),
                rank: idx + 1,
                provider: "demo".to_string(),
                published: Some(published.to_string()),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bse_core::Lang;

    #[tokio::test]
    async fn returns_six_fixtures() {
        let hits = DemoProvider::new()
            .search(&SearchQuery::new("cichy balon", Lang::Pl, 10))
            .await
            .unwrap();
        assert_eq!(hits.len(), 6);
        assert_eq!(hits[0].provider, "demo");
        assert_eq!(hits[0].rank, 1);
    }
}
