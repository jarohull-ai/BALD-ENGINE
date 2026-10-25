use async_trait::async_trait;
use bse_core::{Capabilities, Provider, ProviderError, RawHit, SearchQuery};
use reqwest::Client;
use serde::Deserialize;
use url::Url;

use crate::{http_client, map_reqwest_error, strip_tags};

pub struct SearxngProvider {
    client: Client,
    id: String,
    base_url: Url,
    language: Option<String>,
    safesearch: u8,
}

impl SearxngProvider {
    pub fn new(
        base_url: &str,
        language: Option<String>,
        safesearch: u8,
    ) -> Result<Self, ProviderError> {
        let mut base_url = Url::parse(base_url)
            .map_err(|e| ProviderError::Config(format!("invalid SearXNG URL: {e}")))?;
        if !matches!(base_url.scheme(), "http" | "https") {
            return Err(ProviderError::Config(
                "SearXNG URL must be http(s)".to_string(),
            ));
        }
        if !base_url.path().ends_with('/') {
            let path = format!("{}/", base_url.path());
            base_url.set_path(&path);
        }
        Ok(Self {
            client: http_client()?,
            id: "searxng".to_string(),
            base_url,
            language,
            safesearch: safesearch.min(2),
        })
    }
}

#[derive(Debug, Deserialize)]
struct SearxngResponse {
    #[serde(default, rename = "results")]
    results: Vec<SearxngItem>,
}

#[derive(Debug, Deserialize)]
struct SearxngItem {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    content: String,
    #[serde(default, rename = "publishedDate")]
    published_date: Option<String>,
}

#[async_trait]
impl Provider for SearxngProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn display_name(&self) -> &str {
        "SearXNG"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            web: true,
            news: true,
            archive: false,
            factcheck: false,
            render: false,
        }
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<RawHit>, ProviderError> {
        let endpoint = self
            .base_url
            .join("search")
            .map_err(|e| ProviderError::Config(format!("invalid SearXNG URL: {e}")))?;

        let language = self
            .language
            .clone()
            .or_else(|| query.lang.code().map(str::to_string));

        let mut request = self
            .client
            .get(endpoint)
            .query(&[("q", query.text.as_str()), ("format", "json")])
            .query(&[("safesearch", self.safesearch.to_string())]);

        if let Some(lang) = language {
            request = request.query(&[("language", lang)]);
        }

        let response = request.send().await.map_err(map_reqwest_error)?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(ProviderError::Auth(format!(
                "SearXNG returned {status} (is JSON format enabled?)"
            )));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimited);
        }
        if !status.is_success() {
            return Err(ProviderError::Http(format!("SearXNG returned {status}")));
        }

        let parsed: SearxngResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::Decode(e.to_string()))?;
        Ok(parse_results(parsed))
    }
}

fn parse_results(response: SearxngResponse) -> Vec<RawHit> {
    response
        .results
        .into_iter()
        .filter(|item| !item.url.trim().is_empty())
        .enumerate()
        .map(|(idx, item)| RawHit {
            title: strip_tags(&item.title),
            url: item.url,
            snippet: strip_tags(&item.content),
            rank: idx + 1,
            provider: "searxng".to_string(),
            published: item.published_date,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_searxng_json() {
        let raw = r#"{
            "query": "test",
            "results": [
                {"url":"https://example.org/a","title":"A <b>title</b>","content":"Snippet &amp; more","publishedDate":"2025-01-02"},
                {"url":"","title":"skip","content":""},
                {"url":"https://example.org/b","title":"B","content":""}
            ]
        }"#;
        let parsed: SearxngResponse = serde_json::from_str(raw).unwrap();
        let hits = parse_results(parsed);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "A title");
        assert_eq!(hits[0].snippet, "Snippet & more");
        assert_eq!(hits[1].rank, 2);
        assert_eq!(hits[0].provider, "searxng");
    }
}
