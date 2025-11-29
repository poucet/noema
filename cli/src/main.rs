use std::sync::Arc;

use config::load_env_file;
use llm::{create_model, default_model, get_provider_info, list_providers, ChatModel, ModelId};
use noema_core::{Agent, ConversationContext, MemorySession, SessionStore, SimpleAgent, StorageTransaction};

use clap::Parser as ClapParser;
use clap_derive::{Parser, ValueEnum};
use std::io::{self, BufRead, Write};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[derive(Copy, Clone, ValueEnum, Debug, PartialEq, Eq)]
#[clap(rename_all = "lowercase")]
enum Mode {
    Chat,
    Stream,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Model to use in format "provider/model" (e.g., "claude/claude-sonnet-4-5-20250929")
    /// If only provider is specified, uses that provider's default model.
    #[arg(long, default_value_t = default_model().to_string())]
    model: String,

    #[arg(long, value_enum, default_value_t = Mode::Stream)]
    mode: Mode,

    #[arg(long)]
    system_message: Option<String>,

    #[arg(long, short)]
    tracing: bool,
}

struct AppState {
    session: MemorySession,
    model: Arc<dyn ChatModel + Send + Sync>,
    model_id: ModelId,
    mode: Mode,
}

async fn chat_regular(
    session: &mut MemorySession,
    model: Arc<dyn ChatModel + Send + Sync>,
    message: &str
) -> anyhow::Result<()> {
    let agent = SimpleAgent::new();
    let mut tx = session.begin();
    tx.add(llm::ChatMessage::user(llm::ChatPayload::text(message)));
    agent.execute(&mut tx, model).await?;

    for msg in tx.pending() {
        if msg.role == llm::api::Role::Assistant {
            println!("{}", msg.get_text());
        }
    }

    session.commit(tx).await?;
    Ok(())
}

async fn chat_streaming(
    session: &mut MemorySession,
    model: Arc<dyn ChatModel + Send + Sync>,
    message: &str
) -> anyhow::Result<()> {
    let agent = SimpleAgent::new();
    let mut tx = session.begin();
    tx.add(llm::ChatMessage::user(llm::ChatPayload::text(message)));
    agent.execute_stream(&mut tx, model).await?;

    for msg in tx.pending() {
        if msg.role == llm::api::Role::Assistant {
            print!("{}", msg.get_text());
            io::stdout().flush()?;
        }
    }

    session.commit(tx).await?;
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

fn print_status_bar(model_id: &ModelId) {
    let terminal_width: usize = 80;
    let status = format!(" {} ", model_id);
    let padding = terminal_width.saturating_sub(status.len());
    let left_pad = padding / 2;
    let right_pad = padding - left_pad;

    println!("┌{}┐", "─".repeat(terminal_width - 2));
    println!("│{}{}{}│", " ".repeat(left_pad), status, " ".repeat(right_pad));
    println!("└{}┘", "─".repeat(terminal_width - 2));
}

/// Parse a model string that can be either "provider/model" or just "provider"
fn parse_model_arg(s: &str) -> anyhow::Result<ModelId> {
    if let Some(id) = ModelId::parse(s) {
        return Ok(id);
    }

    // Try as just a provider name
    if let Some(info) = get_provider_info(s) {
        return Ok(ModelId::new(info.name, info.default_model));
    }

    let providers: Vec<_> = list_providers().iter().map(|p| p.name).collect();
    Err(anyhow::anyhow!(
        "Invalid model '{}'. Use 'provider/model' format or just a provider name: {}",
        s,
        providers.join(", ")
    ))
}

mod commands {
    use super::*;

    pub enum Command {
        Quit,
        Help,
        Clear,
        SetModel(String),
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
                        let providers: Vec<_> = list_providers().iter().map(|p| p.name).collect();
                        return Err(format!("Usage: /model <provider/model> or /model <provider>\nAvailable providers: {}", providers.join(", ")));
                    }
                    Ok(Command::SetModel(parts[1].to_string()))
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
                    state.session.messages_mut().clear();
                    println!("Conversation history cleared.");
                    println!();
                    CommandResult::Continue
                }
                Command::SetModel(model_str) => {
                    match parse_model_arg(&model_str) {
                        Ok(new_id) => {
                            match create_model(&new_id.to_string()) {
                                Ok(new_model) => {
                                    state.model = new_model;
                                    state.model_id = new_id;
                                    println!("Switched to {}", state.model_id);
                                    println!("(Conversation history preserved)");
                                }
                                Err(e) => {
                                    eprintln!("Failed to create model: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("{}", e);
                        }
                    }
                    println!();
                    CommandResult::Continue
                }
            }
        }
    }

    fn print_help() {
        let providers: Vec<_> = list_providers().iter().map(|p| p.name).collect();
        println!("Available commands:");
        println!("  /quit, /exit           - Exit the chat");
        println!("  /clear                 - Clear conversation history");
        println!("  /model <provider/model> - Switch model");
        println!("                           Or just /model <provider> for default");
        println!("                           Providers: {}", providers.join(", "));
        println!("  /help                  - Show this help message");
        println!("  Ctrl+D                 - Exit the chat");
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    load_env_file();
    let args = Args::parse();

    setup_tracing(args.tracing);

    let model_id = parse_model_arg(&args.model)?;
    let model = create_model(&model_id.to_string())?;

    let session = if let Some(system_msg) = args.system_message {
        MemorySession::with_system_message(system_msg)
    } else {
        MemorySession::new()
    };

    let mut state = AppState {
        session,
        model,
        model_id,
        mode: args.mode,
    };

    println!();
    println!("Type /help for commands, Ctrl+D or /quit to exit.");
    println!();

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    loop {
        print_status_bar(&state.model_id);
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

        let result = match state.mode {
            Mode::Chat => chat_regular(&mut state.session, state.model.clone(), input).await,
            Mode::Stream => chat_streaming(&mut state.session, state.model.clone(), input).await,
        };

        if let Err(e) = result {
            eprintln!("Error: {}", e);
        }

        println!();
    }

    println!("Conversation had {} messages", state.session.len());
    Ok(())
}
