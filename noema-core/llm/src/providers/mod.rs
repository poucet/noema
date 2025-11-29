pub(crate) mod claude;
pub(crate) mod gemini;
pub(crate) mod ollama;
pub(crate) mod openai;

pub use claude::{ClaudeChatModel, ClaudeProvider};
pub use gemini::{GeminiChatModel, GeminiProvider};
pub use ollama::{OllamaChatModel, OllamaProvider};
pub use openai::{OpenAIChatModel, OpenAIProvider};

use llm_macros::delegate_provider_enum;

#[delegate_provider_enum]
pub enum GeneralModelProvider {
    #[provider(name = "ollama", base_url_env = "OLLAMA_BASE_URL", default_model = "gemma3n:latest")]
    Ollama(OllamaProvider),

    #[provider(name = "gemini", api_key_env = "GEMINI_API_KEY", base_url_env = "GEMINI_BASE_URL", default_model = "models/gemini-2.5-flash")]
    Gemini(GeminiProvider),

    #[provider(name = "claude", api_key_env = "CLAUDE_API_KEY", base_url_env = "CLAUDE_BASE_URL", default_model = "claude-sonnet-4-5-20250929")]
    Claude(ClaudeProvider),

    #[provider(name = "openai", api_key_env = "OPENAI_API_KEY", base_url_env = "OPENAI_BASE_URL", default_model = "gpt-4o-mini")]
    OpenAI(OpenAIProvider),
}
