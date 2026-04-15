//! Mistral embedding implementation.

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
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

pub struct MistralEmbeddingProvider {
    client: Client,
    base_url: String,
    model: String,
    dimensions: usize,
}

impl MistralEmbeddingProvider {
    pub fn new(client: Client, base_url: String, model: String) -> Self {
        // mistral-embed produces 1024-dimensional vectors
        let dimensions = 1024;
        Self {
            client,
            base_url,
            model,
            dimensions,
        }
    }

    fn embeddings_url(&self) -> String {
        format!("{}/embeddings", self.base_url)
    }
}

#[async_trait]
impl EmbeddingProvider for MistralEmbeddingProvider {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Embedding>> {
        let request = EmbeddingRequest {
            model: self.model.clone(),
            input: texts.iter().map(|s| s.to_string()).collect(),
        };

        let response: EmbeddingResponse = self.client.post(self.embeddings_url(), &request).await?;

        Ok(response
            .data
            .into_iter()
            .map(|d| Embedding { vector: d.embedding })
            .collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_id(&self) -> &str {
        &self.model
    }
}
