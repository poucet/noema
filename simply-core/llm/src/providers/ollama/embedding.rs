//! Ollama embedding implementation.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::embedding::{Embedding, EmbeddingProvider};

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    embeddings: Vec<Vec<f32>>,
}

pub struct OllamaEmbeddingProvider {
    client: Client,
    base_url: String,
    model: String,
    dimensions: usize,
}

impl OllamaEmbeddingProvider {
    pub fn new(client: Client, base_url: String, model: String) -> Self {
        // Dimension depends on model; all-minilm is 384, nomic-embed-text is 768.
        // We discover dimensions from the first embed call.
        let dimensions = match model.as_str() {
            "all-minilm" => 384,
            "nomic-embed-text" => 768,
            "mxbai-embed-large" => 1024,
            _ => 384,
        };
        Self {
            client,
            base_url,
            model,
            dimensions,
        }
    }

    fn embeddings_url(&self) -> String {
        format!("{}/api/embed", self.base_url)
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaEmbeddingProvider {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Embedding>> {
        let request = EmbeddingRequest {
            model: self.model.clone(),
            input: texts.iter().map(|s| s.to_string()).collect(),
        };

        let response: EmbeddingResponse =
            self.client.post(self.embeddings_url(), &request).await?;

        Ok(response
            .embeddings
            .into_iter()
            .map(|vector| Embedding { vector })
            .collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_id(&self) -> &str {
        &self.model
    }
}
