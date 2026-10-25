//! Automatyczne wykrywanie lokalnych silników LLM (Ollama, LM Studio, LocalAI).

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredEngine {
    pub id: String,
    pub name: String,
    pub provider_type: String, // "ollama" | "openai"
    pub base_url: String,
    pub available: bool,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryResponse {
    pub engines: Vec<DiscoveredEngine>,
}

#[derive(Deserialize)]
struct OllamaTags {
    #[serde(default)]
    models: Vec<OllamaModelItem>,
}

#[derive(Deserialize)]
struct OllamaModelItem {
    name: String,
}

#[derive(Deserialize)]
struct OpenAiModels {
    #[serde(default)]
    data: Vec<OpenAiModelItem>,
}

#[derive(Deserialize)]
struct OpenAiModelItem {
    id: String,
}

pub async fn discover_local_engines() -> DiscoveryResponse {
    let client = match Client::builder()
        .timeout(Duration::from_millis(1500))
        .connect_timeout(Duration::from_millis(800))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            return DiscoveryResponse {
                engines: Vec::new(),
            }
        }
    };

    let (ollama, lmstudio, localai) = tokio::join!(
        probe_ollama(&client),
        probe_openai_compatible(&client, "lmstudio", "LM Studio", "http://127.0.0.1:1234"),
        probe_openai_compatible(&client, "localai", "LocalAI", "http://127.0.0.1:8080"),
    );

    let mut engines = Vec::new();
    if ollama.available || !ollama.models.is_empty() {
        engines.push(ollama);
    }
    if lmstudio.available || !lmstudio.models.is_empty() {
        engines.push(lmstudio);
    }
    if localai.available || !localai.models.is_empty() {
        engines.push(localai);
    }

    if engines.is_empty() {
        engines.push(DiscoveredEngine {
            id: "ollama".into(),
            name: "Ollama (127.0.0.1:11434)".into(),
            provider_type: "ollama".into(),
            base_url: "http://127.0.0.1:11434".into(),
            available: false,
            models: Vec::new(),
        });
    }

    DiscoveryResponse { engines }
}

async fn probe_ollama(client: &Client) -> DiscoveredEngine {
    let base_url = "http://127.0.0.1:11434";
    let endpoint = format!("{base_url}/api/tags");
    let mut models = Vec::new();
    let mut available = false;

    if let Ok(res) = client.get(&endpoint).send().await {
        if res.status().is_success() {
            available = true;
            if let Ok(tags) = res.json::<OllamaTags>().await {
                models = tags.models.into_iter().map(|m| m.name).collect();
                models.sort();
                models.dedup();
            }
        }
    }

    DiscoveredEngine {
        id: "ollama".into(),
        name: "Ollama".into(),
        provider_type: "ollama".into(),
        base_url: base_url.into(),
        available,
        models,
    }
}

async fn probe_openai_compatible(
    client: &Client,
    id: &str,
    name: &str,
    base_url: &str,
) -> DiscoveredEngine {
    let endpoint = format!("{base_url}/v1/models");
    let mut models = Vec::new();
    let mut available = false;

    if let Ok(res) = client.get(&endpoint).send().await {
        if res.status().is_success() {
            available = true;
            if let Ok(body) = res.json::<OpenAiModels>().await {
                models = body.data.into_iter().map(|m| m.id).collect();
                models.sort();
                models.dedup();
            }
        }
    }

    DiscoveredEngine {
        id: id.into(),
        name: name.into(),
        provider_type: "openai".into(),
        base_url: base_url.into(),
        available,
        models,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn discovery_runs_without_panicking() {
        let resp = discover_local_engines().await;
        assert!(!resp.engines.is_empty());
    }
}
