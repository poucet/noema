use anyhow::Result;
use clap::Parser;
use commands::{CommandHandler, CompletionResult};
use config::load_env_file;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use llm::{create_model, default_model, get_provider_info, list_models, list_providers, ContentBlock, ModelId};
use noema_audio::{VoiceAgent, VoiceCoordinator};
use noema_core::{ChatEngine, EngineEvent, McpRegistry, ServerConfig, SessionStore, SqliteSession, SqliteStore};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame, Terminal,
};
use std::io;
use std::path::PathBuf;

#[cfg(not(debug_assertions))]
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

#[derive(Parser, Debug)]
#[command(name = "noema", about = "AI chat TUI with voice support")]
struct Args {
    #[arg(short = 'w', long, default_value = "base")]
    whisper_model: String,

    #[arg(long)]
    whisper_model_path: Option<PathBuf>,
}

impl Args {
    fn get_whisper_model_path(&self) -> PathBuf {
        if let Some(ref path) = self.whisper_model_path {
            path.clone()
        } else {
            let model_name = match self.whisper_model.as_str() {
                "tiny" => "ggml-tiny.en.bin",
                "base" => "ggml-base.en.bin",
                "small" => "ggml-small.en.bin",
                "medium" => "ggml-medium.en.bin",
                "large" => "ggml-large-v3.bin",
                other => other,
            };
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("noema")
                .join("models")
                .join(model_name)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[commands::completable]
enum McpSubcommand {
    List,
    Add,
    Remove,
    Connect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[commands::completable]
enum ConversationSubcommand {
    New,
    List,
    Load,
    Delete,
    Rename,
}

enum CompletionState {
    Idle,
    Loading,
    Showing { completions: Vec<commands::Completion>, selected: usize },
}

struct InputHistory {
    entries: Vec<String>,
    position: Option<usize>,
    draft: String,
}

impl InputHistory {
    fn new() -> Self {
        Self { entries: Vec::new(), position: None, draft: String::new() }
    }

    fn push(&mut self, entry: String) {
        if !entry.is_empty() && self.entries.last() != Some(&entry) {
            self.entries.push(entry);
        }
        self.position = None;
        self.draft.clear();
    }

    fn prev(&mut self, current_input: &str) -> Option<&str> {
        if self.entries.is_empty() { return None; }
        match self.position {
            None => {
                self.draft = current_input.to_string();
                self.position = Some(self.entries.len() - 1);
            }
            Some(pos) if pos > 0 => { self.position = Some(pos - 1); }
            _ => return None,
        }
        self.position.map(|p| self.entries[p].as_str())
    }

    fn next(&mut self) -> Option<&str> {
        match self.position {
            Some(pos) if pos + 1 < self.entries.len() => {
                self.position = Some(pos + 1);
                Some(&self.entries[pos + 1])
            }
            Some(_) => { self.position = None; Some(&self.draft) }
            None => None,
        }
    }

    fn reset_position(&mut self) {
        self.position = None;
        self.draft.clear();
    }
}

struct App {
    input: Input,
    engine: ChatEngine<SqliteSession>,
    store: SqliteStore,
    current_conversation_id: String,
    current_model_id: ModelId,
    status_message: Option<String>,
    command_output: Option<String>,
    is_streaming: bool,
    thinking_frame: usize,
    current_response: String,
    voice_coordinator: Option<VoiceCoordinator>,
    whisper_model_path: PathBuf,
    scroll_offset: usize,
}

struct AppWithCommands {
    command_handler: CommandHandler<App>,
    completion_state: CompletionState,
    history: InputHistory,
}

fn parse_model_arg(s: &str) -> anyhow::Result<ModelId> {
    if let Some(id) = ModelId::parse(s) { return Ok(id); }
    if let Some(info) = get_provider_info(s) {
        return Ok(ModelId::new(info.name, info.default_model));
    }
    let providers: Vec<_> = list_providers().iter().map(|p| p.name).collect();
    Err(anyhow::anyhow!("Invalid model '{}'. Use 'provider/model' or provider name: {}", s, providers.join(", ")))
}

impl App {
    fn new(model_id: ModelId, system_message: Option<String>, whisper_model_path: PathBuf) -> Result<Self> {
        let model = create_model(&model_id.to_string())?;

        let db_path = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("noema")
            .join("conversations.db");

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let store = SqliteStore::open(&db_path)?;
        let mut session = store.create_conversation()?;
        let conversation_id = session.conversation_id().to_string();

        if let Some(sys_msg) = system_message {
            use llm::{ChatMessage, ChatPayload};
            session.messages_mut().push(ChatMessage::system(ChatPayload::text(sys_msg)));
        }

        let mcp_registry = McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));
        let display_name = model_id.to_string();
        let engine = ChatEngine::new(session, model, display_name, mcp_registry);

        Ok(App {
            input: Input::default(),
            engine,
            store,
            current_conversation_id: conversation_id,
            current_model_id: model_id,
            status_message: None,
            command_output: None,
            is_streaming: false,
            thinking_frame: 0,
            current_response: String::new(),
            voice_coordinator: None,
            whisper_model_path,
            scroll_offset: 0,
        })
    }

    fn switch_session(&mut self, session: SqliteSession) {
        let conversation_id = session.conversation_id().to_string();
        let model = create_model(&self.current_model_id.to_string()).expect("Failed to create model");
        let mcp_registry = McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));
        let display_name = self.current_model_id.to_string();
        self.engine = ChatEngine::new(session, model, display_name, mcp_registry);
        self.current_conversation_id = conversation_id;
        self.scroll_offset = 0;
    }

    fn apply_completion_value(&mut self, value: &str) {
        let current = self.input.value();
        let new_value = if let Some(last_space) = current.rfind(char::is_whitespace) {
            format!("{} {} ", &current[..=last_space].trim(), value)
        } else {
            format!("/{} ", value)
        };
        self.input = Input::from(new_value);
    }

    fn start(&mut self) {}
}

impl AppWithCommands {
    fn new(model_id: ModelId, system_message: Option<String>, whisper_model_path: PathBuf) -> Result<Self> {
        let app = App::new(model_id, system_message, whisper_model_path)?;
        Ok(Self {
            command_handler: CommandHandler::new(app),
            completion_state: CompletionState::Idle,
            history: InputHistory::new(),
        })
    }

    fn start(&mut self) { self.command_handler.target_mut().start(); }

    async fn trigger_completion(&mut self) {
        let input_value = self.command_handler.target().input.value().to_string();
        self.completion_state = CompletionState::Loading;
        let result = self.command_handler.trigger_completion(&input_value).await;
        match result {
            CompletionResult::Completions(completions) => {
                match completions.len() {
                    0 => { self.completion_state = CompletionState::Idle; }
                    1 => {
                        self.completion_state = CompletionState::Idle;
                        self.command_handler.target_mut().apply_completion_value(&completions[0].value);
                    }
                    _ => { self.completion_state = CompletionState::Showing { completions, selected: 0 }; }
                }
            }
            CompletionResult::AutoFilledPrefix { new_input, completions } => {
                self.command_handler.target_mut().input = Input::from(new_input);
                self.completion_state = CompletionState::Showing { completions, selected: 0 };
            }
        }
    }

    async fn handle_command(&mut self, input: &str) -> Result<bool> {
        let result = self.command_handler.execute_command(input).await;
        match result {
            Ok(commands::CommandResult::Success(msg)) => {
                if !msg.is_empty() {
                    if msg.contains('\n') {
                        self.command_handler.target_mut().command_output = Some(msg);
                        self.command_handler.target_mut().status_message = None;
                    } else {
                        self.command_handler.target_mut().status_message = Some(msg);
                        self.command_handler.target_mut().command_output = None;
                    }
                }
                Ok(true)
            }
            Ok(commands::CommandResult::Exit) => Ok(false),
            Err(e) => {
                self.command_handler.target_mut().status_message = Some(format!("Error: {}", e));
                self.command_handler.target_mut().command_output = None;
                Ok(true)
            }
        }
    }

    async fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> Result<bool> {
        if key.kind != KeyEventKind::Press { return Ok(true); }
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Char('d'), KeyModifiers::CONTROL) => Ok(false),
            (KeyCode::Tab, _) => {
                if matches!(self.completion_state, CompletionState::Showing { .. }) {
                    self.next_completion();
                } else {
                    self.trigger_completion().await;
                }
                Ok(true)
            }
            (KeyCode::Down, _) => { self.handle_down(); Ok(true) }
            (KeyCode::Up, _) => { self.handle_up(); Ok(true) }
            (KeyCode::Esc, _) => { self.cancel_completion(); Ok(true) }
            (KeyCode::Enter, _) => {
                if let Some(input_text) = self.handle_enter() {
                    self.command_handler.target_mut().status_message = None;
                    if !input_text.is_empty() {
                        if input_text.starts_with('/') {
                            self.handle_command(&input_text).await
                        } else {
                            self.command_handler.target_mut().queue_message(input_text);
                            Ok(true)
                        }
                    } else { Ok(true) }
                } else { Ok(true) }
            }
            _ => {
                self.cancel_completion();
                self.history.reset_position();
                self.command_handler.target_mut().input.handle_event(&Event::Key(key));
                Ok(true)
            }
        }
    }

    fn next_completion(&mut self) {
        if let CompletionState::Showing { ref mut selected, ref completions } = self.completion_state {
            *selected = (*selected + 1) % completions.len();
        }
    }

    fn prev_completion(&mut self) {
        if let CompletionState::Showing { ref mut selected, ref completions } = self.completion_state {
            *selected = if *selected == 0 { completions.len() - 1 } else { *selected - 1 };
        }
    }

    fn handle_down(&mut self) {
        if matches!(self.completion_state, CompletionState::Showing { .. }) {
            self.next_completion();
        } else if let Some(next) = self.history.next() {
            self.command_handler.target_mut().input = Input::from(next.to_string());
        }
    }

    fn handle_up(&mut self) {
        if matches!(self.completion_state, CompletionState::Showing { .. }) {
            self.prev_completion();
        } else {
            let current_input = self.command_handler.target().input.value().to_string();
            if let Some(prev) = self.history.prev(&current_input) {
                self.command_handler.target_mut().input = Input::from(prev.to_string());
            }
        }
    }

    fn handle_enter(&mut self) -> Option<String> {
        if let CompletionState::Showing { selected, ref completions } = self.completion_state {
            if let Some(completion) = completions.get(selected) {
                let value = completion.value.clone();
                self.completion_state = CompletionState::Idle;
                self.command_handler.target_mut().apply_completion_value(&value);
            }
            return None;
        }
        let input_text = self.command_handler.target_mut().input.value().to_string();
        self.command_handler.target_mut().input.reset();
        self.history.push(input_text.clone());
        Some(input_text)
    }

    fn cancel_completion(&mut self) { self.completion_state = CompletionState::Idle; }
}

#[commands::commandable]
impl App {
    #[command(name = "help", help = "Show available commands")]
    async fn cmd_help(&mut self) -> Result<String, anyhow::Error> {
        let providers: Vec<_> = list_providers().iter().map(|p| p.name).collect();
        Ok(format!("Available commands:
  /help - Show this help
  /clear - Clear conversation history
  /conversation <subcommand> - Manage conversations (new, list, load, delete, rename)
  /model <provider/model> - Switch model (providers: {})
  /mcp <subcommand> - Manage MCP servers
  /voice - Toggle voice input mode
  /quit - Exit", providers.join(", ")))
    }

    #[command(name = "clear", help = "Clear conversation history")]
    async fn cmd_clear(&mut self) -> Result<String, anyhow::Error> {
        if self.is_streaming {
            Err(anyhow::anyhow!("Cannot clear conversation while streaming"))
        } else {
            self.engine.clear_history();
            Ok("Conversation cleared".to_string())
        }
    }

    #[command(name = "quit", help = "Exit the application")]
    async fn cmd_quit(&mut self) -> Result<(), anyhow::Error> { Ok(()) }

    #[command(name = "model", help = "Switch model (provider/model or just provider for default)")]
    async fn cmd_model(&mut self, model_str: String) -> Result<String, anyhow::Error> {
        let new_id = parse_model_arg(&model_str)?;
        let model = create_model(&new_id.to_string())?;
        if self.is_streaming {
            Err(anyhow::anyhow!("Cannot switch model while streaming"))
        } else {
            self.engine.set_model(model, new_id.to_string());
            self.current_model_id = new_id.clone();
            Ok(format!("Switched to {}", new_id))
        }
    }

    #[completer(arg = "model_str")]
    async fn complete_model_str(&self, partial: &str) -> Result<Vec<commands::Completion>, anyhow::Error> {
        let mut completions = Vec::new();
        if let Some((provider_name, model_partial)) = partial.split_once('/') {
            if let Ok(models) = list_models(provider_name).await {
                for model_info in models {
                    if model_info.id.model.to_lowercase().starts_with(&model_partial.to_lowercase()) {
                        completions.push(commands::Completion::simple(model_info.id.to_string()));
                    }
                }
            }
        } else {
            for info in list_providers() {
                if info.name.starts_with(&partial.to_lowercase()) {
                    completions.push(commands::Completion::with_description(
                        info.name.to_string(),
                        format!("default: {}", info.default_model)
                    ));
                }
            }
        }
        Ok(completions)
    }

    #[command(name = "mcp", help = "Manage MCP servers (list, add, remove, connect)")]
    async fn cmd_mcp(&mut self, subcommand: McpSubcommand, arg1: Option<String>, arg2: Option<String>) -> Result<String, anyhow::Error> {
        let registry_arc = self.engine.get_mcp_registry();
        let mut registry = registry_arc.lock().await;
        match subcommand {
            McpSubcommand::List => {
                let servers = registry.list_servers();
                if servers.is_empty() {
                    Ok("No MCP servers configured.\nUse /mcp add <id> <url> to add one.".to_string())
                } else {
                    let mut output = String::from("Configured MCP servers:\n");
                    for (id, config) in servers {
                        let status = if registry.is_connected(id) { "[connected]" } else { "[disconnected]" };
                        output.push_str(&format!("  {} - {} {}\n", id, config.url, status));
                    }
                    Ok(output)
                }
            }
            McpSubcommand::Add => {
                let id = arg1.ok_or_else(|| anyhow::anyhow!("Usage: /mcp add <id> <url>"))?;
                let url = arg2.ok_or_else(|| anyhow::anyhow!("Usage: /mcp add <id> <url>"))?;
                let config = ServerConfig { name: id.clone(), url, auth: Default::default(), use_well_known: false, auth_token: None };
                registry.add_server(id.clone(), config);
                registry.save_config()?;
                Ok(format!("Added MCP server '{}'", id))
            }
            McpSubcommand::Remove => {
                let id = arg1.ok_or_else(|| anyhow::anyhow!("Usage: /mcp remove <id>"))?;
                match registry.remove_server(&id).await? {
                    Some(_) => { registry.save_config()?; Ok(format!("Removed MCP server '{}'", id)) }
                    None => Err(anyhow::anyhow!("Server '{}' not found", id)),
                }
            }
            McpSubcommand::Connect => {
                let id = arg1.ok_or_else(|| anyhow::anyhow!("Usage: /mcp connect <id>"))?;
                match registry.connect(&id).await {
                    Ok(server) => {
                        let tool_names: Vec<String> = server.tools.iter().map(|t| t.name.to_string()).collect();
                        Ok(format!("Connected to '{}' with {} tools: {}", id, server.tools.len(), tool_names.join(", ")))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to connect to '{}': {}", id, e)),
                }
            }
        }
    }

    #[completer(arg = "arg1")]
    async fn complete_mcp_arg1(&self, subcommand: &McpSubcommand, partial: &str) -> Result<Vec<commands::Completion>, anyhow::Error> {
        let registry_arc = self.engine.get_mcp_registry();
        let registry = registry_arc.lock().await;
        match subcommand {
            McpSubcommand::Remove | McpSubcommand::Connect => {
                Ok(registry.list_servers().into_iter()
                    .filter(|(id, _)| id.to_lowercase().starts_with(&partial.to_lowercase()))
                    .map(|(id, cfg)| commands::Completion::with_description(id.to_string(), cfg.url.clone()))
                    .collect())
            }
            _ => Ok(vec![]),
        }
    }

    #[command(name = "voice", help = "Toggle voice input mode")]
    async fn cmd_voice(&mut self) -> Result<String, anyhow::Error> {
        if self.voice_coordinator.is_some() {
            self.voice_coordinator = None;
            Ok("Voice mode disabled".to_string())
        } else {
            match VoiceAgent::new(&self.whisper_model_path) {
                Ok(voice_agent) => {
                    let model_name = self.whisper_model_path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
                    self.voice_coordinator = Some(VoiceCoordinator::new(voice_agent));
                    Ok(format!("Voice mode enabled ({}) - speak to send messages", model_name))
                }
                Err(e) => Err(anyhow::anyhow!("Failed to initialize voice ({}): {}", self.whisper_model_path.display(), e)),
            }
        }
    }

    #[command(name = "conversation", help = "Manage conversations (new, list, load, delete, rename)")]
    async fn cmd_conversation(&mut self, subcommand: ConversationSubcommand, id: Option<String>, name: Option<String>) -> Result<String, anyhow::Error> {
        match subcommand {
            ConversationSubcommand::New => {
                if self.is_streaming { return Err(anyhow::anyhow!("Cannot start new conversation while streaming")); }
                let session = self.store.create_conversation()?;
                let conv_id = session.conversation_id().to_string();
                self.switch_session(session);
                Ok(format!("Started new conversation {}", &conv_id[..8]))
            }
            ConversationSubcommand::List => {
                let conversations = self.store.list_conversations()?;
                if conversations.is_empty() { return Ok("No saved conversations.".to_string()); }
                let mut output = String::from("Saved conversations:\n");
                for info in conversations {
                    let marker = if info.id == self.current_conversation_id { " *" } else { "" };
                    let short_id = if info.id.len() > 8 { &info.id[..8] } else { &info.id };
                    let name_part = info.name.map(|n| format!(" \"{}\"", n)).unwrap_or_default();
                    output.push_str(&format!("  {}{} ({} msgs){}\n", short_id, name_part, info.message_count, marker));
                }
                output.push_str("\nUse /conversation load <id> to load");
                Ok(output)
            }
            ConversationSubcommand::Load => {
                let conversation_id = id.ok_or_else(|| anyhow::anyhow!("Usage: /conversation load <id>"))?;
                if self.is_streaming { return Err(anyhow::anyhow!("Cannot load conversation while streaming")); }
                let conversations = self.store.list_conversations()?;
                let matching: Vec<_> = conversations.iter().filter(|info| info.id.starts_with(&conversation_id)).collect();
                let full_id = match matching.len() {
                    0 => return Err(anyhow::anyhow!("Conversation not found: {}", conversation_id)),
                    1 => matching[0].id.clone(),
                    _ => return Err(anyhow::anyhow!("Ambiguous ID '{}', matches: {}", conversation_id, matching.len())),
                };
                let session = self.store.open_conversation(&full_id)?;
                let msg_count = session.len();
                self.switch_session(session);
                Ok(format!("Loaded conversation {} ({} messages)", &full_id[..8], msg_count))
            }
            ConversationSubcommand::Delete => {
                let conversation_id = id.ok_or_else(|| anyhow::anyhow!("Usage: /conversation delete <id>"))?;
                let conversations = self.store.list_conversations()?;
                let matching: Vec<_> = conversations.iter().filter(|info| info.id.starts_with(&conversation_id)).collect();
                let full_id = match matching.len() {
                    0 => return Err(anyhow::anyhow!("Conversation not found: {}", conversation_id)),
                    1 => matching[0].id.clone(),
                    _ => return Err(anyhow::anyhow!("Ambiguous ID '{}', matches: {}", conversation_id, matching.len())),
                };
                if full_id == self.current_conversation_id {
                    return Err(anyhow::anyhow!("Cannot delete the current conversation. Use /conversation new first."));
                }
                self.store.delete_conversation(&full_id)?;
                Ok(format!("Deleted conversation {}", &full_id[..8]))
            }
            ConversationSubcommand::Rename => {
                let conversation_id = id.ok_or_else(|| anyhow::anyhow!("Usage: /conversation rename <id> [name]"))?;
                let conversations = self.store.list_conversations()?;
                let matching: Vec<_> = conversations.iter().filter(|info| info.id.starts_with(&conversation_id)).collect();
                let full_id = match matching.len() {
                    0 => return Err(anyhow::anyhow!("Conversation not found: {}", conversation_id)),
                    1 => matching[0].id.clone(),
                    _ => return Err(anyhow::anyhow!("Ambiguous ID '{}', matches: {}", conversation_id, matching.len())),
                };
                self.store.rename_conversation(&full_id, name.as_deref())?;
                let short_id = &full_id[..8.min(full_id.len())];
                match name {
                    Some(n) => Ok(format!("Renamed conversation {} to \"{}\"", short_id, n)),
                    None => Ok(format!("Cleared name for conversation {}", short_id)),
                }
            }
        }
    }

    #[completer(command = "conversation", arg = "id")]
    async fn complete_conversation_id(&self, subcommand: &ConversationSubcommand, partial: &str) -> Result<Vec<commands::Completion>, anyhow::Error> {
        match subcommand {
            ConversationSubcommand::Load | ConversationSubcommand::Rename => {
                let conversations = self.store.list_conversations()?;
                Ok(conversations.into_iter()
                    .filter(|info| info.id.starts_with(partial))
                    .map(|info| {
                        let short_id = if info.id.len() > 8 { info.id[..8].to_string() } else { info.id.clone() };
                        let label = info.name.clone().unwrap_or_else(|| short_id.clone());
                        commands::Completion::with_description(short_id, format!("{} msgs", info.message_count)).with_label(label)
                    })
                    .collect())
            }
            ConversationSubcommand::Delete => {
                let conversations = self.store.list_conversations()?;
                Ok(conversations.into_iter()
                    .filter(|info| info.id.starts_with(partial) && info.id != self.current_conversation_id)
                    .map(|info| {
                        let short_id = if info.id.len() > 8 { info.id[..8].to_string() } else { info.id.clone() };
                        let label = info.name.clone().unwrap_or_else(|| short_id.clone());
                        commands::Completion::with_description(short_id, format!("{} msgs", info.message_count)).with_label(label)
                    })
                    .collect())
            }
            _ => Ok(vec![]),
        }
    }
}

impl App {
    fn scroll_up(&mut self, lines: usize) { self.scroll_offset = self.scroll_offset.saturating_add(lines); }
    fn scroll_down(&mut self, lines: usize) { self.scroll_offset = self.scroll_offset.saturating_sub(lines); }

    fn queue_message(&mut self, message: String) {
        self.is_streaming = true;
        self.thinking_frame = 0;
        self.current_response.clear();
        self.engine.send_message(message);
    }

    fn get_thinking_indicator(&self) -> &'static str {
        const BRAILLE_FRAMES: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
        BRAILLE_FRAMES[self.thinking_frame % BRAILLE_FRAMES.len()]
    }

    fn advance_thinking_animation(&mut self) { self.thinking_frame = self.thinking_frame.wrapping_add(1); }

    fn check_engine_events(&mut self) {
        while let Some(event) = self.engine.try_recv() {
            match event {
                EngineEvent::Message(msg) => { self.current_response.push_str(&msg.get_text()); }
                EngineEvent::MessageComplete => { self.is_streaming = false; self.current_response.clear(); }
                EngineEvent::Error(err) => {
                    self.is_streaming = false;
                    self.status_message = Some(format!("Error: {}", err));
                    self.current_response.clear();
                }
                EngineEvent::ModelChanged(_) | EngineEvent::HistoryCleared => {}
            }
        }
    }

    fn check_voice_events(&mut self) {
        if let Some(ref mut coordinator) = self.voice_coordinator {
            let (messages, errors) = coordinator.process(self.is_streaming);
            for msg in messages { self.queue_message(msg); }
            for err in errors { self.status_message = Some(format!("Voice error: {}", err)); }
        }
    }
}

fn ui(f: &mut Frame, app_with_commands: &mut AppWithCommands) {
    let completion_state = &app_with_commands.completion_state;
    let completion_loading = matches!(completion_state, CompletionState::Loading);
    let completion_data: Option<(Vec<commands::Completion>, usize)> = match completion_state {
        CompletionState::Showing { completions, selected } => Some((completions.clone(), *selected)),
        _ => None,
    };

    let app = app_with_commands.command_handler.target_mut();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3), Constraint::Length(1)])
        .split(f.area());

    let history = if let Ok(session) = app.engine.get_session().try_lock() {
        session.messages().to_vec()
    } else { Vec::new() };

    let mut all_lines: Vec<Line> = Vec::new();

    for msg in history.iter() {
        let role = match msg.role {
            llm::Role::User => "You",
            llm::Role::Assistant => app.engine.get_model_name(),
            llm::Role::System => "System",
        };
        let style = match msg.role {
            llm::Role::User => Style::default().fg(Color::Cyan),
            llm::Role::Assistant => Style::default().fg(Color::Green),
            llm::Role::System => Style::default().fg(Color::Yellow),
        };
        all_lines.push(Line::from(Span::styled(format!("[{}]", role), style.add_modifier(Modifier::BOLD))));

        for block in &msg.payload.content {
            match block {
                ContentBlock::Text { text } => {
                    for line in text.lines() {
                        let styled_line = if line.starts_with("```") {
                            Line::from(Span::styled(line.to_string(), Style::default().fg(Color::DarkGray)))
                        } else if line.starts_with("# ") {
                            Line::from(Span::styled(line.to_string(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)))
                        } else if line.starts_with("## ") {
                            Line::from(Span::styled(line.to_string(), Style::default().fg(Color::Yellow)))
                        } else if line.starts_with("- ") || line.starts_with("* ") {
                            Line::from(Span::styled(line.to_string(), Style::default().fg(Color::Cyan)))
                        } else if line.starts_with('`') && line.ends_with('`') {
                            Line::from(Span::styled(line.to_string(), Style::default().fg(Color::Magenta)))
                        } else {
                            Line::from(line.to_string())
                        };
                        all_lines.push(styled_line);
                    }
                }
                ContentBlock::Image { mime_type, data } => {
                    all_lines.push(Line::from(Span::styled(format!("[Image: {} ~{}KB]", mime_type, data.len() / 1024), Style::default().fg(Color::Magenta).add_modifier(Modifier::ITALIC))));
                }
                ContentBlock::Audio { mime_type, data } => {
                    all_lines.push(Line::from(Span::styled(format!("[Audio: {} ~{}KB]", mime_type, data.len() / 1024), Style::default().fg(Color::Blue).add_modifier(Modifier::ITALIC))));
                }
                ContentBlock::ToolCall(call) => {
                    all_lines.push(Line::from(Span::styled(format!("[Tool call: {}]", call.name), Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC))));
                }
                ContentBlock::ToolResult(result) => {
                    all_lines.push(Line::from(Span::styled(format!("[Tool result: {}]", result.tool_call_id), Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC))));
                    for content in &result.content {
                        match content {
                            llm::ToolResultContent::Text { text } => {
                                for line in text.lines() {
                                    all_lines.push(Line::from(Span::styled(line.to_string(), Style::default().fg(Color::DarkGray))));
                                }
                            }
                            llm::ToolResultContent::Image { mime_type, data } => {
                                all_lines.push(Line::from(Span::styled(format!("  [Image: {} ~{}KB]", mime_type, data.len() / 1024), Style::default().fg(Color::Magenta).add_modifier(Modifier::ITALIC))));
                            }
                            llm::ToolResultContent::Audio { mime_type, data } => {
                                all_lines.push(Line::from(Span::styled(format!("  [Audio: {} ~{}KB]", mime_type, data.len() / 1024), Style::default().fg(Color::Blue).add_modifier(Modifier::ITALIC))));
                            }
                        }
                    }
                }
            }
        }
        all_lines.push(Line::from(""));
    }

    if app.is_streaming && !app.current_response.is_empty() {
        all_lines.push(Line::from(Span::styled(format!("[{}]", app.engine.get_model_name()), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))));
        for line in app.current_response.lines() { all_lines.push(Line::from(line.to_string())); }
        all_lines.push(Line::from(""));
    }

    if let Some(ref output) = app.command_output {
        all_lines.push(Line::from(Span::styled("[Command]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))));
        for line in output.lines() { all_lines.push(Line::from(Span::styled(line.to_string(), Style::default().fg(Color::White)))); }
        all_lines.push(Line::from(""));
    }

    let total_lines = all_lines.len();
    let visible_height = chunks[0].height.saturating_sub(2) as usize;
    let max_scroll = total_lines.saturating_sub(visible_height);
    if app.scroll_offset > max_scroll { app.scroll_offset = max_scroll; }
    let effective_scroll = max_scroll.saturating_sub(app.scroll_offset);

    let chat_content = Paragraph::new(all_lines)
        .block(Block::default().borders(Borders::ALL).title("Chat"))
        .scroll((effective_scroll as u16, 0));
    f.render_widget(chat_content, chunks[0]);

    if total_lines > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight).begin_symbol(Some("▲")).end_symbol(Some("▼"));
        let mut scrollbar_state = ScrollbarState::new(max_scroll).position(effective_scroll);
        let scrollbar_area = chunks[0].inner(ratatui::layout::Margin { vertical: 1, horizontal: 0 });
        f.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
    }

    let input_widget = Paragraph::new(app.input.value())
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title("Message (/ for commands)"));
    f.render_widget(input_widget, chunks[1]);

    let message_count = app.engine.get_session().try_lock().map(|s| s.len()).unwrap_or(0);
    let is_listening = app.voice_coordinator.as_ref().map(|v| v.is_listening()).unwrap_or(false);
    let is_transcribing = app.voice_coordinator.as_ref().map(|v| v.is_transcribing()).unwrap_or(false);

    let voice_indicator = if is_listening { " | 🎤 Listening..." } else if app.voice_coordinator.is_some() { " | 🎙️ Voice" } else { "" };
    let status_text = if let Some(ref msg) = app.status_message {
        format!(" {} | {}{} ", app.current_model_id, msg, voice_indicator)
    } else if completion_loading {
        format!(" {} | {} Loading completions...{} ", app.current_model_id, app.get_thinking_indicator(), voice_indicator)
    } else if is_transcribing {
        format!(" {} | {} Transcribing...{} ", app.current_model_id, app.get_thinking_indicator(), voice_indicator)
    } else if app.is_streaming {
        format!(" {} | {} Thinking...{} ", app.current_model_id, app.get_thinking_indicator(), voice_indicator)
    } else {
        format!(" {} | {} messages{} ", app.current_model_id, message_count, voice_indicator)
    };

    let status_bar = Paragraph::new(status_text).style(Style::default().bg(Color::DarkGray).fg(Color::White));
    f.render_widget(status_bar, chunks[2]);

    if let Some((completions, selected)) = completion_data {
        let completion_items: Vec<ListItem> = completions.iter().enumerate().map(|(i, c)| {
            let label = c.label.as_ref().unwrap_or(&c.value);
            let desc = c.description.as_ref().map(|d| format!(" - {}", d)).unwrap_or_default();
            let style = if i == selected { Style::default().bg(Color::Blue).fg(Color::White) } else { Style::default() };
            ListItem::new(format!("{}{}", label, desc)).style(style)
        }).collect();
        let completion_list = List::new(completion_items).block(Block::default().borders(Borders::ALL).title("Completions"));
        let popup_height = (completions.len() as u16 + 2).min(10);
        let popup_y = chunks[1].y.saturating_sub(popup_height);
        let popup_area = ratatui::layout::Rect { x: chunks[1].x, y: popup_y, width: chunks[1].width, height: popup_height };
        f.render_widget(completion_list, popup_area);
    }

    f.set_cursor_position((chunks[1].x + app.input.visual_cursor() as u16 + 1, chunks[1].y + 1));
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let whisper_model_path = args.get_whisper_model_path();

    #[cfg(debug_assertions)]
    let log_file = {
        let path = PathBuf::from("./noema.log");
        let _ = std::fs::remove_file(&path);
        std::fs::File::create(&path)?
    };
    #[cfg(debug_assertions)]
    let (non_blocking, _guard) = tracing_appender::non_blocking(log_file);

    #[cfg(not(debug_assertions))]
    let (non_blocking, _guard) = {
        let log_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("noema").join("logs");
        std::fs::create_dir_all(&log_dir)?;
        let file_appender = RollingFileAppender::new(Rotation::DAILY, &log_dir, "noema.log");
        tracing_appender::non_blocking(file_appender)
    };

    tracing_subscriber::registry().with(fmt::layer().with_writer(non_blocking).with_ansi(false)).init();
    load_env_file();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let model_id = default_model();
    let mut app = AppWithCommands::new(model_id, None, whisper_model_path)?;
    app.start();

    let mut should_quit = false;
    while !should_quit {
        terminal.draw(|f| ui(f, &mut app))?;
        app.command_handler.target_mut().check_engine_events();
        app.command_handler.target_mut().check_voice_events();

        let is_listening = app.command_handler.target().voice_coordinator.as_ref().map(|v| v.is_listening()).unwrap_or(false);
        let is_transcribing = app.command_handler.target().voice_coordinator.as_ref().map(|v| v.is_transcribing()).unwrap_or(false);
        if app.command_handler.target().is_streaming || is_listening || is_transcribing {
            app.command_handler.target_mut().advance_thinking_animation();
        }

        if event::poll(std::time::Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => { should_quit = !app.handle_key_event(key).await?; }
                Event::Mouse(mouse) => {
                    match mouse.kind {
                        MouseEventKind::ScrollUp => { app.command_handler.target_mut().scroll_up(3); }
                        MouseEventKind::ScrollDown => { app.command_handler.target_mut().scroll_down(3); }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}
