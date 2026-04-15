//! Local embedding provider using fastembed (ONNX Runtime).

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Mutex;

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

use crate::embedding::{Embedding, EmbeddingProvider};

pub struct LocalEmbeddingProvider {
    model: Mutex<TextEmbedding>,
    model_id: String,
    dimensions: usize,
}

impl LocalEmbeddingProvider {
    /// Create a local embedding provider. Downloads the model on first use.
    pub fn new(model_name: &str) -> Result<Self> {
        let (embedding_model, dimensions) = match model_name {
            "bge-small-en-v1.5" => (EmbeddingModel::BGESmallENV15, 384),
            "all-minilm-l6-v2" | "all-minilm" => (EmbeddingModel::AllMiniLML6V2, 384),
            "bge-base-en-v1.5" => (EmbeddingModel::BGEBaseENV15, 768),
            "bge-large-en-v1.5" => (EmbeddingModel::BGELargeENV15, 1024),
            _ => {
                tracing::warn!(model = model_name, "unknown local model, defaulting to bge-small-en-v1.5");
                (EmbeddingModel::BGESmallENV15, 384)
            }
        };

        let model = TextEmbedding::try_new(InitOptions::new(embedding_model))?;

        Ok(Self {
            model: Mutex::new(model),
            model_id: format!("local/{}", model_name),
            dimensions,
        })
    }
}

#[async_trait]
impl EmbeddingProvider for LocalEmbeddingProvider {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Embedding>> {
        let texts: Vec<String> = texts.iter().map(|s| s.to_string()).collect();

        // fastembed ONNX inference is fast (~10-30ms per chunk), run synchronously
        let model = self.model.lock().unwrap();
        let vectors = model.embed(texts, None)?;

        Ok(vectors
            .into_iter()
            .map(|vector| Embedding { vector })
            .collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}
