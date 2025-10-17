use clap::Parser;
use llm::{ChatModel, ChatRequest, ModelProvider};
use llm::providers::{GeminiProvider, GeneralModelProvider};
use llm::providers::OllamaProvider;
use futures::{StreamExt};

use clap_derive::{Parser, ValueEnum};
use dotenv;

// Load GEMINI_API_KEY from ~/.env file
fn get_gemini_api_key() -> String {
    let home_dir = if let Some(home) = directories::UserDirs::new() {
        home.home_dir().to_path_buf()
    } else {
        panic!("Could not determine home directory");
    };  
    let env_path = home_dir.join(".env");
    dotenv::from_path(env_path).ok();
    std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY must be set in .env file")
}

#[derive(Clone, ValueEnum, Debug, PartialEq, Eq)]
#[clap(rename_all = "lowercase")]
enum ModelProviderType {
    Ollama,
    Gemini,
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
        ModelProviderType::Gemini => GeneralModelProvider::Gemini(GeminiProvider::default(&get_gemini_api_key())),
    };
    let model_name = match args.model {
         ModelProviderType::Ollama => "gemma3n:latest",
         ModelProviderType::Gemini => "models/gemini-2.5-flash",
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
