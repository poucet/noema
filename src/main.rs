use clap::Parser;
use conversation::Conversation;
use futures::StreamExt;
use llm::providers::OllamaProvider;
use llm::providers::{ClaudeProvider, GeminiProvider, OpenAIProvider, GeneralModelProvider};
use llm::ModelProvider;

use clap_derive::{Parser, ValueEnum};
use dotenv;
use std::io::{self, Write};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

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
    OpenAI,
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

    #[arg(long, value_enum, default_value_t = Mode::Stream)]
    mode: Mode,

    #[arg(long)]
    system_message: Option<String>,

    #[arg(long, short)]
    tracing: bool,
}

async fn chat_regular(
    conversation: &mut Conversation,
    message: &str,
) -> anyhow::Result<()> {
    let response = conversation.send(message).await?;
    println!("{}", response);
    Ok(())
}

async fn chat_streaming(
    conversation: &mut Conversation,
    message: &str,
) -> anyhow::Result<()> {
    let mut stream = conversation.send_stream(message).await?;
    while let Some(chunk) = stream.next().await {
        print!("{}", chunk.content);
        io::stdout().flush()?;
    }
    println!();
    Ok(())
}

fn setup_tracing(enable: bool) {
    if enable {
        let subscriber = FmtSubscriber::builder()
            .with_max_level(Level::TRACE)
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("Setting default subscriber failed");
    } else {
        // Send tracing to /dev/null
        let subscriber = FmtSubscriber::builder()
            .with_max_level(Level::ERROR)
            .with_writer(|| std::io::sink())
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("Setting default subscriber failed");
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    setup_tracing(args.tracing);

    let provider: GeneralModelProvider = match args.model {
        ModelProviderType::Ollama => GeneralModelProvider::Ollama(OllamaProvider::default()),
        ModelProviderType::Gemini => {
            GeneralModelProvider::Gemini(GeminiProvider::default(&get_api_key("GEMINI_API_KEY")))
        }
        ModelProviderType::Claude => {
            GeneralModelProvider::Claude(ClaudeProvider::default(&get_api_key("CLAUDE_API_KEY")))
        }
        ModelProviderType::OpenAI => {
            GeneralModelProvider::OpenAI(OpenAIProvider::default(&get_api_key("OPENAI_API_KEY")))
        }
    };
    let model_name = match args.model {
        ModelProviderType::Ollama => "gemma3n:latest",
        ModelProviderType::Gemini => "models/gemini-2.5-flash",
        ModelProviderType::Claude => "claude-sonnet-4-5-20250929",
        ModelProviderType::OpenAI => "gpt-4o-mini",
    };

    let model = provider.create_chat_model(model_name).unwrap();

    let mut conversation = if let Some(system_msg) = args.system_message {
        Conversation::with_system_message(model, system_msg)
    } else {
        Conversation::new(model)
    };

    println!("Chat started. Type 'exit' or 'quit' to end the conversation.");
    println!();

    loop {
        print!("> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        if input == "exit" || input == "quit" {
            println!("Goodbye!");
            break;
        }

        let result = match args.mode {
            Mode::Chat => chat_regular(&mut conversation, input).await,
            Mode::Stream => chat_streaming(&mut conversation, input).await,
        };

        if let Err(e) = result {
            eprintln!("Error: {}", e);
        }

        println!();
    }

    println!("Conversation had {} messages", conversation.message_count());
}
