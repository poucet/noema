use clap::Parser;
use config::{create_provider, get_model_info, load_env_file, ProviderUrls};
use conversation::Conversation;
use futures::StreamExt;
use llm::ModelProvider;

use clap_derive::{Parser, ValueEnum};
use std::io::{self, BufRead, Write};
use std::str::FromStr;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

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

impl FromStr for ModelProviderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ollama" => Ok(ModelProviderType::Ollama),
            "gemini" => Ok(ModelProviderType::Gemini),
            "claude" => Ok(ModelProviderType::Claude),
            "openai" => Ok(ModelProviderType::OpenAI),
            _ => Err(format!("Unknown provider: {}", s)),
        }
    }
}

impl From<&ModelProviderType> for config::ModelProviderType {
    fn from(p: &ModelProviderType) -> Self {
        match p {
            ModelProviderType::Ollama => config::ModelProviderType::Ollama,
            ModelProviderType::Gemini => config::ModelProviderType::Gemini,
            ModelProviderType::Claude => config::ModelProviderType::Claude,
            ModelProviderType::OpenAI => config::ModelProviderType::OpenAI,
        }
    }
}

#[derive(Copy, Clone, ValueEnum, Debug, PartialEq, Eq)]
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

    /// Custom base URL for OpenAI API (e.g., for proxy or compatible services)
    #[arg(long, env = "OPENAI_BASE_URL")]
    openai_url: Option<String>,

    /// Custom base URL for Claude/Anthropic API (e.g., for proxy)
    #[arg(long, env = "CLAUDE_BASE_URL")]
    claude_url: Option<String>,

    /// Custom base URL for Gemini API (e.g., for proxy)
    #[arg(long, env = "GEMINI_BASE_URL")]
    gemini_url: Option<String>,

    /// Custom base URL for Ollama API
    #[arg(long, env = "OLLAMA_BASE_URL")]
    ollama_url: Option<String>,
}

// Application state
struct AppState {
    conversation: Conversation,
    current_provider: ModelProviderType,
    model_display_name: &'static str,
    mode: Mode,
    provider_urls: ProviderUrls,
}

async fn call_model_regular(
    model: &dyn llm::ChatModel,
    messages: Vec<llm::ChatMessage>,
) -> anyhow::Result<()> {
    let request = llm::ChatRequest::new(messages);
    let response = model.chat(&request).await?;
    println!("Response: {:}", response.get_text());
    Ok(())
}

async fn call_model_streaming(
    model: &impl llm::ChatModel,
    messages: Vec<llm::ChatMessage>,
) -> anyhow::Result<()> {
    let request = llm::ChatRequest::new(messages);
    let mut stream = model.stream_chat(&request).await?;
    print!("Response: ");
    while let Some(chunk) = stream.next().await {
        print!("{:}", chunk.get_text());
    }
    println!("");
    Ok(())
}

async fn chat_regular(conversation: &mut Conversation, message: &str) -> anyhow::Result<()> {
    let response = conversation.send(message).await?;
    println!("{}", response.get_text());
    Ok(())
}

async fn chat_streaming(conversation: &mut Conversation, message: &str) -> anyhow::Result<()> {
    let mut stream = conversation.send_stream(message).await?;
    while let Some(chunk) = stream.next().await {
        print!("{}", chunk.payload.get_text());
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

// Slash command parsing and handling
mod commands {
    use super::*;

    pub enum Command {
        Quit,
        Help,
        Clear,
        SetModel(ModelProviderType),
    }

    pub enum CommandResult {
        Continue,
        Exit,
    }

    impl Command {
        pub fn parse(input: &str) -> Result<Self, String> {
            if !input.starts_with('/') {
                return Err("Not a command".to_string());
            }

            let parts: Vec<&str> = input[1..].split_whitespace().collect();
            if parts.is_empty() {
                return Err("Empty command".to_string());
            }

            match parts[0] {
                "quit" | "exit" => Ok(Command::Quit),
                "help" => Ok(Command::Help),
                "clear" => Ok(Command::Clear),
                "model" => {
                    if parts.len() < 2 {
                        return Err("Usage: /model <provider>".to_string());
                    }
                    ModelProviderType::from_str(parts[1])
                        .map(Command::SetModel)
                        .map_err(|_| format!("Unknown provider: {}. Available: ollama, gemini, claude, openai", parts[1]))
                }
                _ => Err(format!("Unknown command: /{}. Type /help for available commands.", parts[0])),
            }
        }

        pub fn execute(self, state: &mut AppState) -> CommandResult {
            match self {
                Command::Quit => {
                    println!("Goodbye!");
                    CommandResult::Exit
                }
                Command::Help => {
                    print_help();
                    println!();
                    CommandResult::Continue
                }
                Command::Clear => {
                    state.conversation.clear();
                    println!("Conversation history cleared.");
                    println!();
                    CommandResult::Continue
                }
                Command::SetModel(new_provider) => {
                    let config_provider: config::ModelProviderType = (&new_provider).into();
                    let (new_model_id, new_model_display) = get_model_info(&config_provider);
                    let new_provider_instance = create_provider(&config_provider, &state.provider_urls);

                    match new_provider_instance.create_chat_model(new_model_id) {
                        Some(new_model) => {
                            state.conversation.set_model(new_model);
                            state.current_provider = new_provider;
                            state.model_display_name = new_model_display;
                            println!("Switched to {} • {}", state.current_provider, state.model_display_name);
                            println!("(Conversation history preserved)");
                        }
                        None => {
                            eprintln!("Failed to create model for {}", new_provider);
                        }
                    }
                    println!();
                    CommandResult::Continue
                }
            }
        }
    }

    fn print_help() {
        println!("Available commands:");
        println!("  /quit, /exit           - Exit the chat");
        println!("  /clear                 - Clear conversation history");
        println!("  /model <provider>      - Switch model (ollama, gemini, claude, openai)");
        println!("  /help                  - Show this help message");
        println!("  Ctrl+D                 - Exit the chat");
    }
}

#[tokio::main]
async fn main() {
    load_env_file();
    let args = Args::parse();

    setup_tracing(args.tracing);

    let provider_urls = ProviderUrls {
        openai: args.openai_url,
        claude: args.claude_url,
        gemini: args.gemini_url,
        ollama: args.ollama_url,
    };

    if args.tracing {
        eprintln!("DEBUG: Provider URLs: {:?}", provider_urls);
    }

    let config_provider: config::ModelProviderType = (&args.model).into();
    let (model_id, model_display_name) = get_model_info(&config_provider);
    let provider = create_provider(&config_provider, &provider_urls);
    let model = provider.create_chat_model(model_id).unwrap();

    let conversation = if let Some(system_msg) = args.system_message {
        Conversation::with_system_message(model, system_msg)
    } else {
        Conversation::new(model)
    };

    let mut state = AppState {
        conversation,
        current_provider: args.model.clone(),
        model_display_name,
        mode: args.mode,
        provider_urls,
    };

    println!();
    println!("Type /help for commands, Ctrl+D or /quit to exit.");
    println!();

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    loop {
        print_status_bar(&state.current_provider, state.model_display_name);
        print!("> ");
        io::stdout().flush().unwrap();

        let line = match lines.next() {
            Some(Ok(line)) => line,
            Some(Err(e)) => {
                eprintln!("Error reading input: {}", e);
                break;
            }
            None => {
                println!();
                println!("Goodbye!");
                break;
            }
        };

        let input = line.trim();

        if input.is_empty() {
            continue;
        }

        // Try to parse as command
        if input.starts_with('/') {
            match commands::Command::parse(input) {
                Ok(cmd) => {
                    match cmd.execute(&mut state) {
                        commands::CommandResult::Exit => break,
                        commands::CommandResult::Continue => continue,
                    }
                }
                Err(err) => {
                    println!("{}", err);
                    println!();
                    continue;
                }
            }
        }

        // Regular message
        let result = match state.mode {
            Mode::Chat => chat_regular(&mut state.conversation, input).await,
            Mode::Stream => chat_streaming(&mut state.conversation, input).await,
        };

        if let Err(e) = result {
            eprintln!("Error: {}", e);
        }

        println!();
    }

    println!("Conversation had {} messages", state.conversation.message_count());
}
