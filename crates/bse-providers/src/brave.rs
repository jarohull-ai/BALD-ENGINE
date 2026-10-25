use async_trait::async_trait;
use bse_core::{Capabilities, Provider, ProviderError, RawHit, SearchQuery};
use reqwest::Client;
use serde::Deserialize;

use crate::{http_client, map_reqwest_error, strip_tags};

pub struct BraveProvider {
    client: Client,
    api_key: String,
    country: Option<String>,
}

impl BraveProvider {
    pub fn new(api_key: Option<String>, country: Option<String>) -> Result<Self, ProviderError> {
        let api_key = api_key
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| ProviderError::Config("BRAVE_API_KEY is missing".to_string()))?;
        Ok(Self {
            client: http_client()?,
            api_key,
            country,
        })
    }
}

#[derive(Debug, Deserialize)]
struct BraveResponse {
    #[serde(default)]
    web: Option<BraveWeb>,
}

#[derive(Debug, Deserialize)]
struct BraveWeb {
    #[serde(default)]
    results: Vec<BraveItem>,
}

#[derive(Debug, Deserialize)]
struct BraveItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    age: Option<String>,
}

#[async_trait]
impl Provider for BraveProvider {
    fn id(&self) -> &str {
        "brave"
    }

    fn display_name(&self) -> &str {
        "Brave Search"
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
        let count = query.limit.clamp(1, 20).to_string();
        let mut request = self
            .client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("X-Subscription-Token", &self.api_key)
            .header("Accept", "application/json")
            .query(&[("q", query.text.as_str()), ("count", count.as_str())]);

        if let Some(lang) = query.lang.code() {
            request = request.query(&[("search_lang", lang)]);
        }
        if let Some(country) = &self.country {
            request = request.query(&[("country", country.as_str())]);
        }

        let response = request.send().await.map_err(map_reqwest_error)?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(ProviderError::Auth(format!("Brave returned {status}")));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimited);
        }
        if !status.is_success() {
            return Err(ProviderError::Http(format!("Brave returned {status}")));
        }

        let parsed: BraveResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::Decode(e.to_string()))?;
        Ok(parse_results(parsed))
    }
}

fn parse_results(response: BraveResponse) -> Vec<RawHit> {
    response
        .web
        .map(|web| web.results)
        .unwrap_or_default()
        .into_iter()
        .filter(|item| !item.url.trim().is_empty())
        .enumerate()
        .map(|(idx, item)| RawHit {
            title: strip_tags(&item.title),
            url: item.url,
            snippet: strip_tags(&item.description),
            rank: idx + 1,
            provider: "brave".to_string(),
            published: item.age,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_brave_json() {
        let raw = r#"{
            "web": {
                "results": [
                    {"title":"A <strong>title</strong>","url":"https://example.org/a","description":"Desc","age":"2 days ago"},
                    {"title":"B","url":"https://example.org/b","description":"Desc B"}
                ]
            }
        }"#;
        let parsed: BraveResponse = serde_json::from_str(raw).unwrap();
        let hits = parse_results(parsed);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "A title");
        assert_eq!(hits[0].rank, 1);
        assert_eq!(hits[0].provider, "brave");
    }

    #[test]
    fn missing_web_is_empty() {
        let parsed: BraveResponse = serde_json::from_str("{}").unwrap();
        assert!(parse_results(parsed).is_empty());
    }
}
