//! Adaptery providerów wyszukiwania.
//!
//! Faza 0: SearXNG (JSON), Brave Web Search, Demo (offline).
//! Każdy adapter mapuje błędy na `ProviderError`; pipeline nie zna HTTP.

pub mod brave;
pub mod demo;
pub mod searxng;

pub use brave::BraveProvider;
pub use demo::DemoProvider;
pub use searxng::SearxngProvider;

use reqwest::Client;

pub(crate) fn http_client() -> Result<Client, bse_core::ProviderError> {
    Client::builder()
        .user_agent("BALD/0.1 (research; +https://github.com/jarohull-ai/bald)")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| bse_core::ProviderError::Unavailable(e.to_string()))
}

pub(crate) fn map_reqwest_error(err: reqwest::Error) -> bse_core::ProviderError {
    if err.is_timeout() {
        bse_core::ProviderError::Timeout
    } else if err.is_connect() {
        bse_core::ProviderError::Unavailable(err.to_string())
    } else {
        bse_core::ProviderError::Http(err.to_string())
    }
}

/// Proste usunięcie tagów HTML z snippetów. Nie jest to parser DOM —
/// dla wyników wyszukiwania wystarczy; pełna ekstrakcja będzie w `bse-fetch`.
pub(crate) fn strip_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for c in input.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_markup() {
        assert_eq!(
            strip_tags("<b>Cichy</b> balon &amp; <i>aktualizacje</i>"),
            "Cichy balon & aktualizacje"
        );
    }
}
