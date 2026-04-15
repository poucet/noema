//! Local in-process embedding provider via fastembed (ONNX Runtime).
//!
//! Downloads the model from Hugging Face on first use and caches it locally.
//! No API keys, no external services required.

#[cfg(feature = "local-embedding")]
mod embedding;

#[cfg(feature = "local-embedding")]
pub use embedding::LocalEmbeddingProvider;
