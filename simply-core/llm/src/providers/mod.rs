pub(crate) mod claude;
pub(crate) mod gemini;
pub mod local;
pub(crate) mod mistral;
pub(crate) mod ollama;
pub(crate) mod openai;

pub use claude::{ClaudeChatModel, ClaudeProvider};
pub use gemini::{GeminiChatModel, GeminiProvider};
pub use mistral::{MistralChatModel, MistralEmbeddingProvider, MistralProvider};
#[cfg(feature = "local-embedding")]
pub use local::LocalEmbeddingProvider;
pub use ollama::{OllamaChatModel, OllamaEmbeddingProvider, OllamaProvider};
pub use openai::{OpenAIChatModel, OpenAIProvider};

use llm_macros::delegate_provider_enum;

#[delegate_provider_enum]
pub enum GeneralModelProvider {
    #[provider(name = "ollama", base_url_env = "OLLAMA_BASE_URL")]
    Ollama(OllamaProvider),

    #[provider(name = "gemini", api_key_env = "GEMINI_API_KEY", base_url_env = "GEMINI_BASE_URL")]
    Gemini(GeminiProvider),

    #[provider(name = "claude", api_key_env = "CLAUDE_API_KEY", base_url_env = "CLAUDE_BASE_URL")]
    Claude(ClaudeProvider),

    #[provider(name = "openai", api_key_env = "OPENAI_API_KEY", base_url_env = "OPENAI_BASE_URL")]
    OpenAI(OpenAIProvider),

    #[provider(name = "mistral", api_key_env = "MISTRAL_API_KEY", base_url_env = "MISTRAL_BASE_URL")]
    Mistral(MistralProvider),
}
