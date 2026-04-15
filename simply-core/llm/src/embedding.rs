//! Embedding provider trait.
//!
//! Lives in the `llm` crate alongside `ChatModel` and `ModelProvider` because
//! embedding providers share the same API clients, key resolution, and
//! infrastructure as chat providers.

use anyhow::Result;
use async_trait::async_trait;

/// A single embedding vector.
pub struct Embedding {
    pub vector: Vec<f32>,
}

/// Trait for generating embedding vectors from text.
///
/// Implementations exist for API providers (Mistral, OpenAI, Gemini, etc.)
/// and local models (ONNX). The provider is configured server-side in
/// `settings.toml` — switching providers requires re-embedding all vectors.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Embed a batch of texts, returning one vector per input.
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Embedding>>;

    /// The dimensionality of vectors this provider produces.
    fn dimensions(&self) -> usize;

    /// Provider/model identifier (e.g. "mistral/mistral-embed").
    fn model_id(&self) -> &str;
}
