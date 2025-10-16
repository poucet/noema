use llm::{ChatModel, ModelProvider};
use llm::providers::OllamaProvider;
use futures::{StreamExt};
use tokio::pin;

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
    let stream = model.stream_chat(messages).await.unwrap();
    pin!(stream);
    print!("Response: ");
    while let Some(chunk) = stream.next().await {
        print!("{:}", chunk.content);
    }
    println!("");
}
