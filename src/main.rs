use std::fmt::format;

use clap::Parser;
use llm::{ChatModel, ChatRequest, ModelProvider};
use llm::providers::{ClaudeProvider, GeminiProvider, GeneralModelProvider};
use llm::providers::OllamaProvider;
use futures::{StreamExt};

use clap_derive::{Parser, ValueEnum};
use dotenv;

// Load GEMINI_API_KEY from ~/.env file
fn get_api_key(key: &str) -> String {
    let home_dir = if let Some(home) = directories::UserDirs::new() {
        home.home_dir().to_path_buf()
    } else {
        panic!("Could not determine home directory");
    };  
    let env_path = home_dir.join(".env");
    dotenv::from_path(env_path).ok();
    std::env::var(key).expect(&format!("{:} must be set in .env file", key))
}

#[derive(Clone, ValueEnum, Debug, PartialEq, Eq)]
#[clap(rename_all = "lowercase")]
enum ModelProviderType {
    Ollama,
    Gemini,
    Claude,
}

#[derive(Clone, ValueEnum, Debug, PartialEq, Eq)]
#[clap(rename_all = "lowercase")]
enum Mode {
    Chat,
    Stream,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, value_enum, default_value_t = ModelProviderType::Gemini)]
    model: ModelProviderType,

    #[arg(long, value_enum, default_value_t = Mode::Chat)]
    mode: Mode,
}

async fn call_model_regular(model: &dyn ChatModel, messages: Vec<llm::ChatMessage>) -> anyhow::Result<()> {
    let request = ChatRequest::new(messages);
    let response = model.chat(&request).await?;
    println!("Response: {:}", response.content);
    Ok(())
}

async fn call_model_streaming(model: &impl ChatModel, messages: Vec<llm::ChatMessage> ) -> anyhow::Result<()> {
    let request = ChatRequest::new(messages);
    let mut stream = model.stream_chat(&request).await?;
    print!("Response: ");
    while let Some(chunk) = stream.next().await {
        print!("{:}", chunk.content);
    }
    println!("");
    Ok(())
}

#[tokio::main]
async fn main() {
    let args = Args::try_parse().ok().unwrap();
    let provider: GeneralModelProvider = match args.model {
        ModelProviderType::Ollama => GeneralModelProvider::Ollama(OllamaProvider::default()),
        ModelProviderType::Gemini => GeneralModelProvider::Gemini(GeminiProvider::default(&get_api_key("GEMINI_API_KEY"))),
        ModelProviderType::Claude => GeneralModelProvider::Claude(ClaudeProvider::default(&get_api_key("CLAUDE_API_KEY")))
    };
    let model_name = match args.model {
         ModelProviderType::Ollama => "gemma3n:latest",
         ModelProviderType::Gemini => "models/gemini-2.5-flash",
         ModelProviderType::Claude => "claude-sonnet-4-5-20250929",
    };
    let models = provider.list_models().await;
    println!("Available models: {:?}", models);

    let model = provider.create_chat_model(model_name).unwrap();
    let messages = vec![
        llm::ChatMessage {
            role: llm::Role::User,
            content: "Hello, please return a long meaningful message!".to_string(),
        },
    ];
    match args.mode {
        Mode::Chat => call_model_regular(&model, messages).await.unwrap(),
        Mode::Stream => call_model_streaming(&model, messages).await.unwrap()
    }
}
