pub mod chat;
pub mod embedding;
mod provider;

pub use chat::model::OllamaChatModel;
pub use embedding::OllamaEmbeddingProvider;
pub use provider::OllamaProvider;
