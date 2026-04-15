//! Claude/Voyage embedding implementation.
//!
//! Anthropic partners with Voyage AI for embeddings. This provider calls
//! the Voyage API (OpenAI-compatible format) using the Anthropic API key.

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

pub struct VoyageEmbeddingProvider {
    client: Client,
    model: String,
    dimensions: usize,
}

impl VoyageEmbeddingProvider {
    pub fn new(api_key: &str, model: String) -> Self {
        use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key))
                .expect("Invalid API key format"),
        );

        let dimensions = match model.as_str() {
            "voyage-3-lite" => 512,
            "voyage-3" => 1024,
            "voyage-3-large" => 2048,
            _ => 1024,
        };

        Self {
            client: Client::with_headers(headers),
            model,
            dimensions,
        }
    }
}

#[async_trait]
impl EmbeddingProvider for VoyageEmbeddingProvider {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Embedding>> {
        let request = EmbeddingRequest {
            model: self.model.clone(),
            input: texts.iter().map(|s| s.to_string()).collect(),
        };

        let response: EmbeddingResponse = self.client
            .post("https://api.voyageai.com/v1/embeddings", &request)
            .await?;

        Ok(response.data.into_iter()
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
