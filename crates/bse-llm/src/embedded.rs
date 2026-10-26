//! Wbudowany, lokalny silnik wnioskowania LLM (GGUF / Candle) dla BALD.
//!
//! Zapewnia natychmiastowe działanie Out-of-the-Box w języku polskim i angielskim
//! bez konieczności uruchamiania zewnętrznych serwisów (np. Ollama).

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use async_trait::async_trait;
use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::quantized_qwen2::ModelWeights;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokenizers::Tokenizer;

use crate::{Llm, LlmError};

/// Informacje o certyfikowanym modelu w katalogu BALD.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub filename: String,
    pub tokenizer_filename: String,
    pub size_bytes: u64,
    pub download_url: String,
    pub tokenizer_url: String,
    pub context_size: usize,
}

/// Domyślny katalog modeli wspieranych natywnie w BALD.
pub struct ModelCatalog;

impl ModelCatalog {
    pub const DEFAULT_MODEL_ID: &'static str = "qwen2.5-0.5b-instruct";
    pub const RECOMMENDED_MODEL_ID: &'static str = "qwen2.5-1.5b-instruct";

    pub fn available_models() -> Vec<ModelPreset> {
        vec![
            ModelPreset {
                id: "qwen2.5-0.5b-instruct".to_string(),
                name: "Qwen 2.5 0.5B Instruct (Ultra-Light)".to_string(),
                description: "Niezwykle lekki model (398 MB) — błyskawiczny start i minimalne zużycie RAM/VRAM na każdym CPU."
                    .to_string(),
                filename: "qwen2.5-0.5b-instruct-q4_k_m.gguf".to_string(),
                tokenizer_filename: "qwen2.5-0.5b-tokenizer.json".to_string(),
                size_bytes: 398_000_000,
                download_url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf"
                    .to_string(),
                tokenizer_url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct/raw/main/tokenizer.json"
                    .to_string(),
                context_size: 4096,
            },
            ModelPreset {
                id: "qwen2.5-1.5b-instruct".to_string(),
                name: "Qwen 2.5 1.5B Instruct (Zalecany)".to_string(),
                description: "Złoty standard dla BALD (986 MB) — wysoka precyzja wyciągania cytatów, wsparcie narzędzi i płynna polszczyzna."
                    .to_string(),
                filename: "qwen2.5-1.5b-instruct-q4_k_m.gguf".to_string(),
                tokenizer_filename: "qwen2.5-1.5b-tokenizer.json".to_string(),
                size_bytes: 986_000_000,
                download_url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf"
                    .to_string(),
                tokenizer_url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct/raw/main/tokenizer.json"
                    .to_string(),
                context_size: 8192,
            },
            ModelPreset {
                id: "smollm2-1.7b-instruct".to_string(),
                name: "SmolLM2 1.7B Instruct".to_string(),
                description: "Nowoczesny, lekki model badawczy Hugging Face zoptymalizowany pod kątem faktografii (1.05 GB)."
                    .to_string(),
                filename: "smollm2-1.7b-instruct-q4_k_m.gguf".to_string(),
                tokenizer_filename: "smollm2-1.7b-tokenizer.json".to_string(),
                size_bytes: 1_050_000_000,
                download_url: "https://huggingface.co/HuggingFaceTB/SmolLM2-1.7B-Instruct-GGUF/resolve/main/smollm2-1.7b-instruct-q4_k_m.gguf"
                    .to_string(),
                tokenizer_url: "https://huggingface.co/HuggingFaceTB/SmolLM2-1.7B-Instruct/raw/main/tokenizer.json"
                    .to_string(),
                context_size: 4096,
            },
            ModelPreset {
                id: "qwen2.5-3b-instruct".to_string(),
                name: "Qwen 2.5 3B Instruct (Maksymalna wiedza)".to_string(),
                description: "Maksymalna głębia analityczna i wnioskowanie (1.95 GB) dla komputerów z >4 GB RAM."
                    .to_string(),
                filename: "qwen2.5-3b-instruct-q4_k_m.gguf".to_string(),
                tokenizer_filename: "qwen2.5-3b-tokenizer.json".to_string(),
                size_bytes: 1_950_000_000,
                download_url: "https://huggingface.co/Qwen/Qwen2.5-3B-Instruct-GGUF/resolve/main/qwen2.5-3b-instruct-q4_k_m.gguf"
                    .to_string(),
                tokenizer_url: "https://huggingface.co/Qwen/Qwen2.5-3B-Instruct/raw/main/tokenizer.json"
                    .to_string(),
                context_size: 8192,
            },
        ]
    }

    pub fn find(id: &str) -> Option<ModelPreset> {
        let needle = id.trim().to_ascii_lowercase();
        Self::available_models()
            .into_iter()
            .find(|m| m.id.to_ascii_lowercase() == needle)
    }

    pub fn default_preset() -> ModelPreset {
        Self::find(Self::DEFAULT_MODEL_ID).expect("default preset exists")
    }
}

/// Menedżer przechowywania i pobierania wag modeli na dysku.
#[derive(Debug, Clone)]
pub struct ModelStorage {
    base_dir: PathBuf,
}

impl Default for ModelStorage {
    fn default() -> Self {
        Self::new(default_models_dir())
    }
}

impl ModelStorage {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn model_dir(&self, preset: &ModelPreset) -> PathBuf {
        self.base_dir.join(&preset.id)
    }

    pub fn weights_path(&self, preset: &ModelPreset) -> PathBuf {
        self.model_dir(preset).join(&preset.filename)
    }

    pub fn tokenizer_path(&self, preset: &ModelPreset) -> PathBuf {
        self.model_dir(preset).join(&preset.tokenizer_filename)
    }

    pub fn is_installed(&self, preset: &ModelPreset) -> bool {
        let weights = self.weights_path(preset);
        let tokenizer = self.tokenizer_path(preset);
        weights.exists()
            && tokenizer.exists()
            && std::fs::metadata(&weights).map(|m| m.len() > 100_000).unwrap_or(false)
    }

    pub fn is_installed_id(&self, id: &str) -> bool {
        ModelCatalog::find(id)
            .map(|preset| self.is_installed(&preset))
            .unwrap_or(false)
    }

    /// Pobiera wagi modelu oraz plik tokenizera bezpośrednio z Hugging Face.
    pub async fn download<F>(&self, preset: &ModelPreset, progress_cb: F) -> Result<(), LlmError>
    where
        F: Fn(u64, u64) + Send + Sync + 'static,
    {
        let target_dir = self.model_dir(preset);
        tokio::fs::create_dir_all(&target_dir)
            .await
            .map_err(|e| LlmError::Embedded(format!("cannot create model directory: {e}")))?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .build()
            .map_err(LlmError::Http)?;

        // 1. Pobierz tokenizer jeśli brak
        let tok_path = self.tokenizer_path(preset);
        if !tok_path.exists() {
            let resp = client
                .get(&preset.tokenizer_url)
                .send()
                .await
                .map_err(LlmError::Http)?;
            let bytes = resp.bytes().await.map_err(LlmError::Http)?;
            tokio::fs::write(&tok_path, bytes)
                .await
                .map_err(|e| LlmError::Embedded(format!("cannot save tokenizer: {e}")))?;
        }

        // 2. Pobierz wagi GGUF strumieniowo z raportowaniem postępu
        let weights_path = self.weights_path(preset);
        let tmp_path = weights_path.with_extension("tmp");

        let resp = client
            .get(&preset.download_url)
            .send()
            .await
            .map_err(LlmError::Http)?;

        let total_size = resp.content_length().unwrap_or(preset.size_bytes);
        let mut downloaded: u64 = 0;

        let mut file = File::create(&tmp_path)
            .map_err(|e| LlmError::Embedded(format!("cannot create temp file: {e}")))?;

        let mut stream = resp.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(LlmError::Http)?;
            file.write_all(&chunk)
                .map_err(|e| LlmError::Embedded(format!("cannot write chunk: {e}")))?;
            downloaded += chunk.len() as u64;
            progress_cb(downloaded, total_size);
        }

        file.flush()
            .map_err(|e| LlmError::Embedded(format!("cannot flush file: {e}")))?;
        drop(file);

        std::fs::rename(&tmp_path, &weights_path)
            .map_err(|e| LlmError::Embedded(format!("cannot rename model weights: {e}")))?;

        Ok(())
    }
}

/// Domyślny katalog na modele: `~/.local/share/bald/models/`
pub fn default_models_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("bald")
            .join("models")
    } else {
        PathBuf::from("models")
    }
}

struct EmbeddedLlmInner {
    preset: ModelPreset,
    weights: Mutex<ModelWeights>,
    tokenizer: Tokenizer,
    device: Device,
    temperature: f64,
    top_p: f64,
}

impl EmbeddedLlmInner {
    fn generate_sync(&self, prompt: &str, max_tokens: usize) -> Result<String, LlmError> {
        let encoding = self
            .tokenizer
            .encode(prompt, true)
            .map_err(|e| LlmError::Embedded(format!("tokenization error: {e}")))?;

        let prompt_tokens = encoding.get_ids();
        if prompt_tokens.is_empty() {
            return Err(LlmError::Empty);
        }

        let mut weights = self
            .weights
            .lock()
            .map_err(|_| LlmError::Embedded("model mutex poisoned".to_string()))?;

        weights.clear_kv_cache();

        let mut logits_processor =
            LogitsProcessor::new(299792458, Some(self.temperature), Some(self.top_p));

        let eos_tokens = [
            self.tokenizer.token_to_id("<|im_end|>").unwrap_or(151645),
            self.tokenizer.token_to_id("<|endoftext|>").unwrap_or(151643),
            151645,
            151643,
        ];
        let is_eos = |token: u32| eos_tokens.contains(&token);

        let mut generated_tokens: Vec<u32> = Vec::new();

        // 1. Prefill / Prompt evaluation
        let input_tensor = Tensor::new(prompt_tokens, &self.device)
            .map_err(|e| LlmError::Embedded(format!("tensor error: {e}")))?
            .unsqueeze(0)
            .map_err(|e| LlmError::Embedded(format!("tensor unsqueeze error: {e}")))?;

        let logits = weights
            .forward(&input_tensor, 0)
            .map_err(|e| LlmError::Embedded(format!("model forward error: {e}")))?;

        let logits = logits
            .squeeze(0)
            .map_err(|e| LlmError::Embedded(format!("logits squeeze error: {e}")))?;

        let mut next_token = logits_processor
            .sample(&logits)
            .map_err(|e| LlmError::Embedded(format!("sampling error: {e}")))?;

        if is_eos(next_token) {
            weights.clear_kv_cache();
            return Ok(String::new());
        }

        generated_tokens.push(next_token);

        // 2. Autoregressive token generation
        for pos in 0..max_tokens {
            let input = Tensor::new(&[next_token], &self.device)
                .map_err(|e| LlmError::Embedded(format!("tensor token error: {e}")))?
                .unsqueeze(0)
                .map_err(|e| LlmError::Embedded(format!("tensor unsqueeze error: {e}")))?;

            let cur_pos = prompt_tokens.len() + pos;
            let logits = weights
                .forward(&input, cur_pos)
                .map_err(|e| LlmError::Embedded(format!("forward token error: {e}")))?;

            let logits = logits
                .squeeze(0)
                .map_err(|e| LlmError::Embedded(format!("logits squeeze error: {e}")))?;

            next_token = logits_processor
                .sample(&logits)
                .map_err(|e| LlmError::Embedded(format!("sample error: {e}")))?;

            if is_eos(next_token) {
                break;
            }

            generated_tokens.push(next_token);
        }

        weights.clear_kv_cache();

        let output_text = self
            .tokenizer
            .decode(&generated_tokens, true)
            .map_err(|e| LlmError::Decode(format!("detokenization error: {e}")))?;

        Ok(output_text.trim().to_string())
    }
}

static MODEL_CACHE: OnceLock<RwLock<HashMap<String, EmbeddedLlm>>> = OnceLock::new();

fn model_cache() -> &'static RwLock<HashMap<String, EmbeddedLlm>> {
    MODEL_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Wbudowany silnik wnioskowania LLM oparty na Candle i formacie GGUF.
#[derive(Clone)]
pub struct EmbeddedLlm {
    inner: Arc<EmbeddedLlmInner>,
    max_tokens: usize,
}

impl EmbeddedLlm {
    /// Ładuje model GGUF z dysku do pamięci (lub zwraca już załadowaną instancję z cache).
    pub fn load(storage: &ModelStorage, preset: ModelPreset) -> Result<Self, LlmError> {
        let cache = model_cache();
        if let Ok(guard) = cache.read() {
            if let Some(existing) = guard.get(&preset.id) {
                return Ok(existing.clone());
            }
        }

        let weights_path = storage.weights_path(&preset);
        let tokenizer_path = storage.tokenizer_path(&preset);

        if !weights_path.exists() || !tokenizer_path.exists() {
            return Err(LlmError::ModelNotInstalled(format!(
                "Model {} nie jest zainstalowany pod ścieżką {}",
                preset.id,
                weights_path.display()
            )));
        }

        let device = Device::Cpu;

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| LlmError::Embedded(format!("błąd ładowania tokenizera: {e}")))?;

        let mut file = File::open(&weights_path)
            .map_err(|e| LlmError::Embedded(format!("błąd otwierania wag GGUF: {e}")))?;

        let content = gguf_file::Content::read(&mut file)
            .map_err(|e| LlmError::Embedded(format!("błąd czytania nagłówka GGUF: {e}")))?;

        let weights = ModelWeights::from_gguf(content, &mut file, &device)
            .map_err(|e| LlmError::Embedded(format!("błąd inicjalizacji modelu Candle: {e}")))?;

        let instance = Self {
            inner: Arc::new(EmbeddedLlmInner {
                preset: preset.clone(),
                weights: Mutex::new(weights),
                tokenizer,
                device,
                temperature: 0.2,
                top_p: 0.9,
            }),
            max_tokens: 450,
        };

        if let Ok(mut guard) = cache.write() {
            guard.insert(preset.id, instance.clone());
        }

        Ok(instance)
    }

    /// Konfiguruje maksymalną liczbę generowanych tokenów dla tej instancji.
    pub fn with_max_tokens(mut self, max: usize) -> Self {
        self.max_tokens = max.max(32);
        self
    }

    /// Pomocnicza metoda generowania z formatowaniem ChatML.
    pub fn format_chatml(system: &str, user: &str) -> String {
        format!(
            "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
            system.trim(),
            user.trim()
        )
    }
}

#[async_trait]
impl Llm for EmbeddedLlm {
    fn model(&self) -> &str {
        &self.inner.preset.id
    }

    async fn complete(&self, system: &str, user: &str) -> Result<String, LlmError> {
        let prompt = Self::format_chatml(system, user);
        let inner = Arc::clone(&self.inner);
        let max_tokens = self.max_tokens;

        tokio::task::spawn_blocking(move || inner.generate_sync(&prompt, max_tokens))
            .await
            .map_err(|e| LlmError::Embedded(format!("blocking task failed: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_catalog_and_presets_work() {
        let models = ModelCatalog::available_models();
        assert!(models.len() >= 2);

        let default_m = ModelCatalog::default_preset();
        assert_eq!(default_m.id, ModelCatalog::DEFAULT_MODEL_ID);

        let qwen_1_5 = ModelCatalog::find("qwen2.5-1.5b-instruct").expect("qwen 1.5b preset");
        assert_eq!(qwen_1_5.filename, "qwen2.5-1.5b-instruct-q4_k_m.gguf");
        assert!(qwen_1_5.download_url.contains("huggingface.co"));
    }

    #[test]
    fn model_storage_resolves_paths_correctly() {
        let temp_dir = std::env::temp_dir().join("bald_test_models");
        let storage = ModelStorage::new(temp_dir.clone());
        let preset = ModelCatalog::default_preset();

        let m_dir = storage.model_dir(&preset);
        assert_eq!(m_dir, temp_dir.join(&preset.id));

        let w_path = storage.weights_path(&preset);
        assert_eq!(w_path, m_dir.join(&preset.filename));

        let t_path = storage.tokenizer_path(&preset);
        assert_eq!(t_path, m_dir.join(&preset.tokenizer_filename));

        assert!(!storage.is_installed(&preset));
    }

    #[test]
    fn chatml_formatting_is_valid() {
        let formatted = EmbeddedLlm::format_chatml("Jesteś BALD.", "Zbadaj to.");
        assert_eq!(
            formatted,
            "<|im_start|>system\nJesteś BALD.<|im_end|>\n<|im_start|>user\nZbadaj to.<|im_end|>\n<|im_start|>assistant\n"
        );
    }

    #[tokio::test]
    async fn live_inference_if_installed() {
        let storage = ModelStorage::default();
        let preset = ModelCatalog::default_preset();
        if storage.is_installed(&preset) {
            let llm = EmbeddedLlm::load(&storage, preset).expect("load installed model");
            let reply = llm
                .complete("Odpowiadaj krótko.", "Napisz jedno słowo: Cześć")
                .await
                .expect("inference should succeed");
            println!("Real model generation output: {reply}");
            assert!(!reply.trim().is_empty());
        }
    }
}
