use llm::{ChatModel, ModelProvider} ;
use llm::providers::OllamaProvider;

#[tokio::main]
async fn main() {
    let provider = OllamaProvider::default();
    let models = provider.list_models().await;
    println!("Available models: {:?}", models);

    let model = provider.create_chat_model("gemma3n:latest").unwrap();
    let messages = vec![
        llm::ChatMessage {
            role: llm::Role::User,
            content: "Hello, Ollama!".to_string(),
        },
    ];
    let response = model.chat(messages).await;
    println!("Model response: {:?}", response);
}
