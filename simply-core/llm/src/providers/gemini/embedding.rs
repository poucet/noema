//! Gemini embedding implementation.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::client::Client;
use crate::embedding::{Embedding, EmbeddingProvider};

#[derive(Serialize)]
struct EmbeddingRequest {
    requests: Vec<EmbedContentRequest>,
}

#[derive(Serialize)]
struct EmbedContentRequest {
    model: String,
    content: ContentPart,
}

#[derive(Serialize)]
struct ContentPart {
    parts: Vec<TextPart>,
}

#[derive(Serialize)]
struct TextPart {
    text: String,
}

#[derive(Deserialize)]
struct BatchEmbeddingResponse {
    embeddings: Vec<EmbeddingValues>,
}

#[derive(Deserialize)]
struct EmbeddingValues {
    values: Vec<f32>,
}

pub struct GeminiEmbeddingProvider {
    client: Client,
    base_url: String,
    model: String,
    dimensions: usize,
}

impl GeminiEmbeddingProvider {
    pub fn new(client: Client, base_url: String, model: String) -> Self {
        let dimensions = match model.as_str() {
            "text-embedding-004" => 768,
            _ => 768,
        };
        Self { client, base_url, model, dimensions }
    }
}

#[async_trait]
impl EmbeddingProvider for GeminiEmbeddingProvider {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Embedding>> {
        let requests: Vec<EmbedContentRequest> = texts.iter().map(|text| {
            EmbedContentRequest {
                model: format!("models/{}", self.model),
                content: ContentPart {
                    parts: vec![TextPart { text: text.to_string() }],
                },
            }
        }).collect();

        let url = format!("{}/models/{}:batchEmbedContents", self.base_url, self.model);
        let response: BatchEmbeddingResponse = self.client.post(&url, &EmbeddingRequest { requests }).await?;

        Ok(response.embeddings.into_iter()
            .map(|e| Embedding { vector: e.values })
            .collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_id(&self) -> &str {
        &self.model
    }
}
