use async_trait::async_trait;
use bse_core::{Capabilities, Provider, ProviderError, RawHit, SearchQuery};
use reqwest::Client;
use serde::Deserialize;

use crate::{http_client, map_reqwest_error, strip_tags};

pub struct WikipediaProvider {
    client: Client,
    lang: String,
}

impl WikipediaProvider {
    pub fn new(lang: impl Into<String>) -> Result<Self, ProviderError> {
        Ok(Self {
            client: http_client()?,
            lang: lang.into(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct WikiResponse {
    #[serde(default)]
    query: Option<WikiQuery>,
}

#[derive(Debug, Deserialize)]
struct WikiQuery {
    #[serde(default)]
    search: Vec<WikiItem>,
}

#[derive(Debug, Deserialize)]
struct WikiItem {
    title: String,
    snippet: String,
}

#[async_trait]
impl Provider for WikipediaProvider {
    fn id(&self) -> &str {
        "wikipedia"
    }

    fn display_name(&self) -> &str {
        "Wikipedia"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            web: true,
            news: false,
            archive: false,
            factcheck: false,
            render: false,
        }
    }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<RawHit>, ProviderError> {
        let endpoint = format!("https://{}.wikipedia.org/w/api.php", self.lang);
        let limit_str = query.limit.to_string();
        let resp = self
            .client
            .get(&endpoint)
            .query(&[
                ("action", "query"),
                ("list", "search"),
                ("srsearch", &query.text),
                ("format", "json"),
                ("srlimit", &limit_str),
            ])
            .send()
            .await
            .map_err(map_reqwest_error)?;

        if !resp.status().is_success() {
            return Err(ProviderError::Http(format!(
                "Wikipedia HTTP {}",
                resp.status()
            )));
        }

        let body: WikiResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Http(format!("cannot parse Wikipedia JSON: {e}")))?;

        let mut hits = Vec::new();
        if let Some(q) = body.query {
            for (idx, item) in q.search.into_iter().enumerate() {
                let encoded_title = item.title.replace(' ', "_");
                let url = format!(
                    "https://{}.wikipedia.org/wiki/{}",
                    self.lang,
                    url::form_urlencoded::byte_serialize(encoded_title.as_bytes())
                        .collect::<String>()
                );
                hits.push(RawHit {
                    title: item.title,
                    url,
                    snippet: strip_tags(&item.snippet),
                    rank: idx + 1,
                    provider: "wikipedia".to_string(),
                    published: None,
                });
            }
        }

        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wikipedia_json() {
        let json = r#"{
            "query": {
                "search": [
                    {
                        "title": "Bunkier",
                        "snippet": "Schron lub <b>bunkier</b> to zamknięta budowla."
                    }
                ]
            }
        }"#;
        let body: WikiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(body.query.unwrap().search[0].title, "Bunkier");
    }
}
