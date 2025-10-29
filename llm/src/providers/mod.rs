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
    Ollama(OllamaProvider),
    Gemini(GeminiProvider),
    Claude(ClaudeProvider),
    OpenAI(OpenAIProvider),
}
