use async_trait::async_trait;
use bse_core::{Capabilities, Provider, ProviderError, RawHit, SearchQuery};
use regex::Regex;
use reqwest::Client;
use std::sync::OnceLock;

use crate::{map_reqwest_error, strip_tags};

const DDG_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

pub struct DuckDuckGoProvider {
    client: Client,
}

impl DuckDuckGoProvider {
    pub fn new() -> Result<Self, ProviderError> {
        let client = Client::builder()
            .user_agent(DDG_USER_AGENT)
            .timeout(std::time::Duration::from_secs(12))
            .build()
            .map_err(|e| ProviderError::Unavailable(e.to_string()))?;
        Ok(Self { client })
    }
}

fn link_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"class="result__a"[^>]*href="([^"]+)"[^>]*>([\s\S]*?)</a>"#).unwrap()
    })
}

fn snippet_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"class="result__snippet"[^>]*>([\s\S]*?)</a>"#).unwrap())
}

pub fn decode_ddg_url(raw: &str) -> String {
    if let Some(pos) = raw.find("uddg=") {
        let after = &raw[pos + 5..];
        let enc = match after.find('&') {
            Some(end) => &after[..end],
            None => after,
        };
        for (k, v) in url::form_urlencoded::parse(format!("u={enc}").as_bytes()) {
            if k == "u" {
                return v.into_owned();
            }
        }
    }
    raw.to_string()
}

#[async_trait]
impl Provider for DuckDuckGoProvider {
    fn id(&self) -> &str {
        "duckduckgo"
    }

    fn display_name(&self) -> &str {
        "DuckDuckGo (Live Web)"
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
        let url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            url::form_urlencoded::byte_serialize(query.text.as_bytes()).collect::<String>()
        );

        let response = self
            .client
            .get(&url)
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .header("Accept-Language", "pl,en-US;q=0.7,en;q=0.3")
            .send()
            .await
            .map_err(map_reqwest_error)?;

        if !response.status().is_success() {
            return Err(ProviderError::Http(format!(
                "DuckDuckGo HTTP {}",
                response.status()
            )));
        }

        let html = response
            .text()
            .await
            .map_err(|e| ProviderError::Decode(e.to_string()))?;

        let links: Vec<(String, String)> = link_regex()
            .captures_iter(&html)
            .map(|cap| (cap[1].to_string(), strip_tags(&cap[2])))
            .collect();

        let snippets: Vec<String> = snippet_regex()
            .captures_iter(&html)
            .map(|cap| strip_tags(&cap[1]))
            .collect();

        let mut hits = Vec::new();
        for (idx, (raw_href, raw_title)) in links.into_iter().enumerate() {
            if idx >= query.limit {
                break;
            }
            let direct_url = decode_ddg_url(&raw_href);
            if !direct_url.starts_with("http://") && !direct_url.starts_with("https://") {
                continue;
            }

            let snippet = snippets.get(idx).cloned().unwrap_or_default();
            hits.push(RawHit {
                title: raw_title,
                url: direct_url,
                snippet,
                rank: idx + 1,
                provider: "duckduckgo".to_string(),
                published: None,
            });
        }

        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_ddg_redirect_urls() {
        let enc = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Farticle%3Fid%3D1&rut=xyz";
        assert_eq!(decode_ddg_url(enc), "https://example.com/article?id=1");
    }

    #[test]
    fn parses_html_results_snippet() {
        let html = r#"
            <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fnews.org%2Fstory">News Title</a>
            <a class="result__snippet" href="...">This is a live snippet of text.</a>
        "#;
        let links: Vec<(String, String)> = link_regex()
            .captures_iter(html)
            .map(|cap| (cap[1].to_string(), strip_tags(&cap[2])))
            .collect();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].1, "News Title");
        let direct = decode_ddg_url(&links[0].0);
        assert_eq!(direct, "https://news.org/story");
    }
}
