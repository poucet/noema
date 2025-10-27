use clap::Parser;
use conversation::Conversation;
use futures::StreamExt;
use llm::providers::OllamaProvider;
use llm::providers::{ClaudeProvider, GeminiProvider, OpenAIProvider, GeneralModelProvider};
use llm::ModelProvider;

use clap_derive::{Parser, ValueEnum};
use dotenv;
use std::io::{self, BufRead, Write};
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

impl std::fmt::Display for ModelProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
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

fn get_model_info(provider_type: &ModelProviderType) -> (&'static str, &'static str) {
    match provider_type {
        ModelProviderType::Ollama => ("gemma3n:latest", "gemma3n:latest"),
        ModelProviderType::Gemini => ("models/gemini-2.5-flash", "gemini-2.5-flash"),
        ModelProviderType::Claude => ("claude-sonnet-4-5-20250929", "claude-sonnet-4-5"),
        ModelProviderType::OpenAI => ("gpt-4o-mini", "gpt-4o-mini"),
    }
}

fn create_provider(provider_type: &ModelProviderType) -> GeneralModelProvider {
    match provider_type {
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
    }
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

fn print_status_bar(model_provider: &ModelProviderType, model_name: &str) {
    let terminal_width: usize = 80;
    let status = format!(" {} • {} ", model_provider, model_name);
    let padding = terminal_width.saturating_sub(status.len());
    let left_pad = padding / 2;
    let right_pad = padding - left_pad;

    println!("┌{}┐", "─".repeat(terminal_width - 2));
    println!("│{}{}{}│", " ".repeat(left_pad), status, " ".repeat(right_pad));
    println!("└{}┘", "─".repeat(terminal_width - 2));
}

fn handle_slash_command(command: &str) -> Option<SlashCommand> {
    if !command.starts_with('/') {
        return None;
    }

    let parts: Vec<&str> = command[1..].split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    match parts[0] {
        "quit" | "exit" => Some(SlashCommand::Quit),
        "help" => Some(SlashCommand::Help),
        "clear" => Some(SlashCommand::Clear),
        "model" => {
            if parts.len() < 2 {
                Some(SlashCommand::ModelMissing)
            } else {
                let model_str = parts[1].to_lowercase();
                let model = match model_str.as_str() {
                    "ollama" => Some(ModelProviderType::Ollama),
                    "gemini" => Some(ModelProviderType::Gemini),
                    "claude" => Some(ModelProviderType::Claude),
                    "openai" => Some(ModelProviderType::OpenAI),
                    _ => None,
                };
                match model {
                    Some(m) => Some(SlashCommand::SetModel(m)),
                    None => Some(SlashCommand::UnknownModel(model_str)),
                }
            }
        }
        _ => Some(SlashCommand::Unknown(parts[0].to_string())),
    }
}

enum SlashCommand {
    Quit,
    Help,
    Clear,
    SetModel(ModelProviderType),
    ModelMissing,
    UnknownModel(String),
    Unknown(String),
}

fn print_help() {
    println!("Available commands:");
    println!("  /quit, /exit           - Exit the chat");
    println!("  /clear                 - Clear conversation history");
    println!("  /model <provider>      - Switch model (ollama, gemini, claude, openai)");
    println!("  /help                  - Show this help message");
    println!("  Ctrl+D                 - Exit the chat");
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    setup_tracing(args.tracing);

    let mut current_provider = args.model.clone();
    let (model_id, model_display_name) = get_model_info(&current_provider);
    let provider = create_provider(&current_provider);
    let model = provider.create_chat_model(model_id).unwrap();

    let mut conversation = if let Some(system_msg) = args.system_message {
        Conversation::with_system_message(model, system_msg)
    } else {
        Conversation::new(model)
    };

    println!();
    println!("Type /help for commands, Ctrl+D or /quit to exit.");
    println!();

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    loop {
        // Print status bar at bottom (before prompt)
        print_status_bar(&current_provider, model_display_name);
        print!("> ");
        io::stdout().flush().unwrap();

        let line = match lines.next() {
            Some(Ok(line)) => line,
            Some(Err(e)) => {
                eprintln!("Error reading input: {}", e);
                break;
            }
            None => {
                // Ctrl+D pressed (EOF)
                println!();
                println!("Goodbye!");
                break;
            }
        };

        let input = line.trim();

        if input.is_empty() {
            continue;
        }

        // Handle slash commands
        if let Some(cmd) = handle_slash_command(input) {
            match cmd {
                SlashCommand::Quit => {
                    println!("Goodbye!");
                    break;
                }
                SlashCommand::Help => {
                    print_help();
                    println!();
                    continue;
                }
                SlashCommand::Clear => {
                    conversation.clear();
                    println!("Conversation history cleared.");
                    println!();
                    continue;
                }
                SlashCommand::SetModel(new_provider) => {
                    let (new_model_id, new_model_display) = get_model_info(&new_provider);
                    let new_provider_instance = create_provider(&new_provider);

                    match new_provider_instance.create_chat_model(new_model_id) {
                        Some(new_model) => {
                            conversation.set_model(new_model);
                            current_provider = new_provider;
                            println!("Switched to {} • {}", current_provider, new_model_display);
                            println!("(Conversation history preserved)");
                        }
                        None => {
                            eprintln!("Failed to create model for {}", new_provider);
                        }
                    }
                    println!();
                    continue;
                }
                SlashCommand::ModelMissing => {
                    println!("Usage: /model <provider>");
                    println!("Available providers: ollama, gemini, claude, openai");
                    println!();
                    continue;
                }
                SlashCommand::UnknownModel(model) => {
                    println!("Unknown model provider: {}", model);
                    println!("Available providers: ollama, gemini, claude, openai");
                    println!();
                    continue;
                }
                SlashCommand::Unknown(cmd) => {
                    println!("Unknown command: /{}", cmd);
                    println!("Type /help for available commands.");
                    println!();
                    continue;
                }
            }
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
