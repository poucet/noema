use anyhow::Result;
use clap::Parser;
use commands::{CommandHandler, CompletionResult};
use config::{create_provider, get_model_info, load_env_file, ProviderUrls};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use llm::{ContentBlock, ModelProvider};
use noema_audio::{VoiceAgent, VoiceCoordinator};
use noema_core::{ChatEngine, EngineEvent, McpRegistry, ServerConfig, Session};
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
    /// Whisper model for voice transcription (tiny, base, small, medium, large)
    #[arg(short = 'w', long, default_value = "base")]
    whisper_model: String,

    /// Custom path to whisper model file
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
                other => other, // Allow custom model names
            };
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("noema")
                .join("models")
                .join(model_name)
        }
    }
}

/// Local ModelProviderType with #[commands::completable] for tab completion.
/// Converts to config::ModelProviderType for actual provider creation.
#[derive(Clone, Debug, PartialEq, Eq)]
#[commands::completable]
enum ModelProviderType {
    /// Local LLM server
    Ollama,
    /// Google's Gemini models
    Gemini,
    /// Anthropic's Claude models
    Claude,
    /// OpenAI GPT models
    OpenAI,
}

impl std::fmt::Display for ModelProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
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

/// MCP subcommands for /mcp command
#[derive(Clone, Debug, PartialEq, Eq)]
#[commands::completable]
enum McpSubcommand {
    /// List configured MCP servers
    List,
    /// Add a new MCP server
    Add,
    /// Remove an MCP server
    Remove,
    /// Connect to an MCP server
    Connect,
}

enum CompletionState {
    Idle,
    Loading,
    Showing { completions: Vec<commands::Completion>, selected: usize },
}

/// Input history for up/down arrow navigation
struct InputHistory {
    entries: Vec<String>,
    position: Option<usize>,
    draft: String,
}

impl InputHistory {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            position: None,
            draft: String::new(),
        }
    }

    /// Add an entry to history
    fn push(&mut self, entry: String) {
        if !entry.is_empty() && self.entries.last() != Some(&entry) {
            self.entries.push(entry);
        }
        self.position = None;
        self.draft.clear();
    }

    /// Navigate to previous entry (up arrow)
    fn prev(&mut self, current_input: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }

        match self.position {
            None => {
                // Save current input as draft before navigating
                self.draft = current_input.to_string();
                self.position = Some(self.entries.len() - 1);
            }
            Some(pos) if pos > 0 => {
                self.position = Some(pos - 1);
            }
            _ => return None,
        }

        self.position.map(|p| self.entries[p].as_str())
    }

    /// Navigate to next entry (down arrow)
    fn next(&mut self) -> Option<&str> {
        match self.position {
            Some(pos) if pos + 1 < self.entries.len() => {
                self.position = Some(pos + 1);
                Some(&self.entries[pos + 1])
            }
            Some(_) => {
                // Return to draft
                self.position = None;
                Some(&self.draft)
            }
            None => None,
        }
    }

    /// Reset position (called when user types)
    fn reset_position(&mut self) {
        self.position = None;
        self.draft.clear();
    }
}

struct App {
    input: Input,
    engine: ChatEngine,
    current_provider: ModelProviderType,
    status_message: Option<String>,
    is_streaming: bool,
    thinking_frame: usize,
    current_response: String,
    provider_urls: ProviderUrls,
    voice_coordinator: Option<VoiceCoordinator>,
    whisper_model_path: PathBuf,
    scroll_offset: usize,
}

/// Wrapper that owns App and CommandRegistry to avoid borrow checker issues
struct AppWithCommands {
    command_handler: CommandHandler<App>,
    completion_state: CompletionState,
    history: InputHistory,
}

impl App {
    fn new(provider: ModelProviderType, system_message: Option<String>, provider_urls: ProviderUrls, whisper_model_path: PathBuf) -> Result<Self> {
        let config_provider: config::ModelProviderType = (&provider).into();
        let (model_id, model_display_name) = get_model_info(&config_provider);
        let provider_instance = create_provider(&config_provider, &provider_urls);
        let model = provider_instance.create_chat_model(model_id).unwrap();

        let session = if let Some(sys_msg) = system_message {
            Session::with_system_message(sys_msg)
        } else {
            Session::new()
        };

        let mcp_registry = McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));

        let engine = ChatEngine::new(session, model, model_display_name.to_string(), mcp_registry);

        Ok(App {
            input: Input::default(),
            engine,
            current_provider: provider,
            status_message: None,
            is_streaming: false,
            thinking_frame: 0,
            current_response: String::new(),
            provider_urls,
            voice_coordinator: None,
            whisper_model_path,
            scroll_offset: 0,
        })
    }

    /// Apply a completion value to the input
    fn apply_completion_value(&mut self, value: &str) {
        let current = self.input.value();
        let new_value = if let Some(last_space) = current.rfind(char::is_whitespace) {
            format!("{} {} ", &current[..=last_space].trim(), value)
        } else {
            format!("/{} ", value)
        };
        self.input = Input::from(new_value);
    }

    fn start(&mut self) {
        // Engine starts itself in constructor
    }
}

impl AppWithCommands {
    fn new(provider: ModelProviderType, system_message: Option<String>, provider_urls: ProviderUrls, whisper_model_path: PathBuf) -> Result<Self> {
        let app = App::new(provider, system_message, provider_urls, whisper_model_path)?;
        let command_handler = CommandHandler::new(app);

        Ok(Self {
            command_handler,
            completion_state: CompletionState::Idle,
            history: InputHistory::new(),
        })
    }

    fn start(&mut self) {
        self.command_handler.target_mut().start();
    }

    /// Trigger tab completion using the registry
    async fn trigger_completion(&mut self) {
        let input_value = self.command_handler.target().input.value().to_string();
        self.completion_state = CompletionState::Loading;

        let result = self.command_handler.trigger_completion(&input_value).await;

        match result {
            CompletionResult::Completions(completions) => {
                match completions.len() {
                    0 => {
                        self.completion_state = CompletionState::Idle;
                    }
                    1 => {
                        self.completion_state = CompletionState::Idle;
                        self.command_handler.target_mut().apply_completion_value(&completions[0].value);
                    }
                    _ => {
                        self.completion_state = CompletionState::Showing {
                            completions,
                            selected: 0,
                        };
                    }
                }
            }
            CompletionResult::AutoFilledPrefix { new_input, completions } => {
                self.command_handler.target_mut().input = Input::from(new_input);
                self.completion_state = CompletionState::Showing {
                    completions,
                    selected: 0,
                };
            }
        }
    }

    /// Handle a command - returns false if should quit
    async fn handle_command(&mut self, input: &str) -> Result<bool> {
        let result = self.command_handler.execute_command(input).await;

        match result {
            Ok(commands::CommandResult::Success(msg)) => {
                if !msg.is_empty() {
                    self.command_handler.target_mut().status_message = Some(msg);
                }
                Ok(true)
            }
            Ok(commands::CommandResult::Exit) => Ok(false),
            Err(e) => {
                self.command_handler.target_mut().status_message = Some(format!("Error: {}", e));
                Ok(true)
            }
        }
    }

    /// Handle a key event - returns false if should quit
    async fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> Result<bool> {
        if key.kind != KeyEventKind::Press {
            return Ok(true);
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                Ok(false)
            }
            (KeyCode::Tab, _) => {
                if matches!(self.completion_state, CompletionState::Showing { .. }) {
                    self.next_completion();
                } else {
                    self.trigger_completion().await;
                }
                Ok(true)
            }
            (KeyCode::Down, _) => {
                self.handle_down();
                Ok(true)
            }
            (KeyCode::Up, _) => {
                self.handle_up();
                Ok(true)
            }
            (KeyCode::Esc, _) => {
                self.cancel_completion();
                Ok(true)
            }
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
                    } else {
                        Ok(true)
                    }
                } else {
                    Ok(true)
                }
            }
            _ => {
                self.cancel_completion();
                self.history.reset_position();
                self.command_handler.target_mut().input.handle_event(&Event::Key(key));
                Ok(true)
            }
        }
    }

    /// Select next completion
    fn next_completion(&mut self) {
        if let CompletionState::Showing { ref mut selected, ref completions } = self.completion_state {
            *selected = (*selected + 1) % completions.len();
        }
    }

    /// Select previous completion
    fn prev_completion(&mut self) {
        if let CompletionState::Showing { ref mut selected, ref completions } = self.completion_state {
            *selected = if *selected == 0 {
                completions.len() - 1
            } else {
                *selected - 1
            };
        }
    }

    /// Handle down arrow
    fn handle_down(&mut self) {
        if matches!(self.completion_state, CompletionState::Showing { .. }) {
            self.next_completion();
        } else if let Some(next) = self.history.next() {
            self.command_handler.target_mut().input = Input::from(next.to_string());
        }
    }

    /// Handle up arrow
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

    /// Handle enter key
    fn handle_enter(&mut self) -> Option<String> {
        if let CompletionState::Showing { selected, ref completions } = self.completion_state {
            if let Some(completion) = completions.get(selected) {
                let value = completion.value.clone();
                self.completion_state = CompletionState::Idle;
                self.command_handler.target_mut().apply_completion_value(&value);
            }
            return None;
        }

        // Not showing completions - return input for processing
        let input_text = self.command_handler.target_mut().input.value().to_string();
        self.command_handler.target_mut().input.reset();

        // Add to history
        self.history.push(input_text.clone());

        Some(input_text)
    }

    /// Cancel completion
    fn cancel_completion(&mut self) {
        self.completion_state = CompletionState::Idle;
    }
}

// New command system commands
#[commands::commandable]
impl App {
    #[command(name = "help", help = "Show available commands")]
    async fn cmd_help(&mut self) -> Result<String, anyhow::Error> {
        Ok("Available commands:\n  /help - Show this help\n  /clear - Clear conversation\n  /model <provider> - Switch model provider\n  /mcp <subcommand> - Manage MCP servers\n  /voice - Toggle voice input mode\n  /quit - Exit".to_string())
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
    async fn cmd_quit(&mut self) -> Result<(), anyhow::Error> {
        Ok(())
    }

    #[command(name = "model", help = "Switch model provider and optionally select model")]
    async fn cmd_model(
        &mut self,
        provider: ModelProviderType,
        model_name: Option<String>
    ) -> Result<String, anyhow::Error> {
        let config_provider: config::ModelProviderType = (&provider).into();
        let (default_model_id, model_display_name) = get_model_info(&config_provider);
        let provider_instance = create_provider(&config_provider, &self.provider_urls);

        let model_id = model_name.as_deref().unwrap_or(default_model_id);
        let model = provider_instance.create_chat_model(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model '{}' not found for provider {:?}", model_id, provider))?;

        if self.is_streaming {
            Err(anyhow::anyhow!("Cannot switch model while streaming"))
        } else {
            self.engine.set_model(model, model_display_name.to_string());
            self.current_provider = provider;
            Ok(format!("Switched to {} • {}", self.current_provider, model_id))
        }
    }

    #[completer(arg = "model_name")]
    async fn complete_model_name(
        &self,
        provider: &ModelProviderType,
        partial: &str
    ) -> Result<Vec<commands::Completion>, anyhow::Error> {
        let config_provider: config::ModelProviderType = provider.into();
        let provider_instance = create_provider(&config_provider, &self.provider_urls);
        let models = provider_instance.list_models().await?;

        // Filter to only text-capable models and match partial
        Ok(models
            .into_iter()
            .filter(|m| {
                // Only include models that support text/chat
                if !m.has_capability(&llm::ModelCapability::Text) {
                    return false;
                }
                // Only include models that can be created
                if provider_instance.create_chat_model(&m.id).is_none() {
                    return false;
                }
                // Match partial input
                m.id.to_lowercase().starts_with(&partial.to_lowercase())
            })
            .map(|m| commands::Completion::simple(m.id))
            .collect())
    }

    #[command(name = "mcp", help = "Manage MCP servers (list, add, remove, connect)")]
    async fn cmd_mcp(
        &mut self,
        subcommand: McpSubcommand,
        arg1: Option<String>,
        arg2: Option<String>,
    ) -> Result<String, anyhow::Error> {
        // Lock the registry via the engine
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
                        let status = if registry.is_connected(id) {
                            "[connected]"
                        } else {
                            "[disconnected]"
                        };
                        output.push_str(&format!("  {} - {} {}\n", id, config.url, status));
                    }
                    Ok(output)
                }
            }
            McpSubcommand::Add => {
                let id = arg1.ok_or_else(|| anyhow::anyhow!("Usage: /mcp add <id> <url>"))?;
                let url = arg2.ok_or_else(|| anyhow::anyhow!("Usage: /mcp add <id> <url>"))?;

                let config = ServerConfig {
                    name: id.clone(),
                    url,
                    auth_token: None,
                };

                registry.add_server(id.clone(), config);
                registry.save_config()?;

                Ok(format!("Added MCP server '{}'", id))
            }
            McpSubcommand::Remove => {
                let id = arg1.ok_or_else(|| anyhow::anyhow!("Usage: /mcp remove <id>"))?;

                match registry.remove_server(&id).await? {
                    Some(_) => {
                        registry.save_config()?;
                        Ok(format!("Removed MCP server '{}'", id))
                    }
                    None => Err(anyhow::anyhow!("Server '{}' not found", id)),
                }
            }
            McpSubcommand::Connect => {
                let id = arg1.ok_or_else(|| anyhow::anyhow!("Usage: /mcp connect <id>"))?;

                match registry.connect(&id).await {
                    Ok(server) => {
                        let tool_count = server.tools.len();
                        let tool_names: Vec<String> = server.tools.iter().map(|t| t.name.to_string()).collect();
                        Ok(format!(
                            "Connected to '{}' with {} tools: {}",
                            id,
                            tool_count,
                            tool_names.join(", ")
                        ))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to connect to '{}': {}", id, e)),
                }
            }
        }
    }

    #[completer(arg = "arg1")]
    async fn complete_mcp_arg1(
        &self,
        subcommand: &McpSubcommand,
        partial: &str,
    ) -> Result<Vec<commands::Completion>, anyhow::Error> {
        let registry_arc = self.engine.get_mcp_registry();
        let registry = registry_arc.lock().await;
        match subcommand {
            McpSubcommand::Remove | McpSubcommand::Connect => {
                // Complete with existing server IDs
                Ok(registry
                    .list_servers()
                    .into_iter()
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
            // Disable voice mode
            tracing::info!("Disabling voice mode");
            self.voice_coordinator = None;
            Ok("Voice mode disabled".to_string())
        } else {
            // Enable voice mode
            tracing::info!("Enabling voice mode with model: {}", self.whisper_model_path.display());
            match VoiceAgent::new(&self.whisper_model_path) {
                Ok(voice_agent) => {
                    let model_name = self.whisper_model_path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");
                    self.voice_coordinator = Some(VoiceCoordinator::new(voice_agent));
                    tracing::info!("Voice mode enabled successfully");
                    Ok(format!("Voice mode enabled ({}) - speak to send messages", model_name))
                }
                Err(e) => {
                    tracing::error!("Failed to initialize voice: {}", e);
                    Err(anyhow::anyhow!("Failed to initialize voice ({}): {}", self.whisper_model_path.display(), e))
                }
            }
        }
    }
}

impl App {
    fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
    }

    fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

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

    fn advance_thinking_animation(&mut self) {
        self.thinking_frame = self.thinking_frame.wrapping_add(1);
    }

    fn check_engine_events(&mut self) {
        while let Some(event) = self.engine.try_recv() {
            match event {
                EngineEvent::Token(chunk) => {
                    self.current_response.push_str(&chunk);
                }
                EngineEvent::MessageComplete => {
                    self.is_streaming = false;
                    self.current_response.clear();
                }
                EngineEvent::Error(err) => {
                    self.is_streaming = false;
                    self.status_message = Some(format!("Error: {}", err));
                    self.current_response.clear();
                }
                EngineEvent::ModelChanged(_) => {
                }
                EngineEvent::HistoryCleared => {
                    // No cached history to clear
                }
            }
        }
    }

    fn check_voice_events(&mut self) {
        // Collect events via coordinator
        if let Some(ref mut coordinator) = self.voice_coordinator {
            let (messages, errors) = coordinator.process(self.is_streaming);
            
            for msg in messages {
                tracing::info!("TUI: Processing voice message: {:?}", msg);
                self.queue_message(msg);
            }
            
            for err in errors {
                tracing::error!("TUI: Voice error: {}", err);
                self.status_message = Some(format!("Voice error: {}", err));
            }
        }
    }
}

fn ui(f: &mut Frame, app_with_commands: &mut AppWithCommands) {
    // Extract completion_state first (it's Copy-able via the enum)
    let completion_state = &app_with_commands.completion_state;
    let completion_loading = matches!(completion_state, CompletionState::Loading);
    let completion_data: Option<(Vec<commands::Completion>, usize)> = match completion_state {
        CompletionState::Showing { completions, selected } => Some((completions.clone(), *selected)),
        _ => None,
    };

    let app = app_with_commands.command_handler.target_mut();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),      // Chat area
            Constraint::Length(3),   // Input area
            Constraint::Length(1),   // Status bar
        ])
        .split(f.area());

    // Render chat messages
    // Fetch fresh history from session (now possible because Session is not locked during streaming)
    let history = if let Ok(session) = app.engine.get_session().try_lock() {
        session.messages().to_vec()
    } else {
        // If for some reason it IS locked, return empty (shouldn't happen with new design)
        Vec::new()
    };

    // Build all lines for the chat content
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

        // Add role header
        all_lines.push(Line::from(Span::styled(
            format!("[{}]", role),
            style.add_modifier(Modifier::BOLD),
        )));

        // Render each content block
        for block in &msg.payload.content {
            match block {
                ContentBlock::Text { text } => {
                    // Add content lines with basic markdown styling
                    for line in text.lines() {
                        let styled_line = if line.starts_with("```") {
                            Line::from(Span::styled(line.to_string(), Style::default().fg(Color::DarkGray)))
                        } else if line.starts_with("# ") {
                            Line::from(Span::styled(
                                line.to_string(),
                                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                            ))
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
                    // Show image placeholder with size info
                    let size_kb = data.len() / 1024;
                    all_lines.push(Line::from(Span::styled(
                        format!("[Image: {} ~{}KB]", mime_type, size_kb),
                        Style::default().fg(Color::Magenta).add_modifier(Modifier::ITALIC),
                    )));
                }
                ContentBlock::Audio { mime_type, data } => {
                    // Show audio placeholder with size info
                    // TODO: Add audio playback via noema_audio
                    let size_kb = data.len() / 1024;
                    all_lines.push(Line::from(Span::styled(
                        format!("[Audio: {} ~{}KB]", mime_type, size_kb),
                        Style::default().fg(Color::Blue).add_modifier(Modifier::ITALIC),
                    )));
                }
                ContentBlock::ToolCall(call) => {
                    all_lines.push(Line::from(Span::styled(
                        format!("[Tool call: {}]", call.name),
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC),
                    )));
                }
                ContentBlock::ToolResult(result) => {
                    all_lines.push(Line::from(Span::styled(
                        format!("[Tool result: {}]", result.tool_call_id),
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC),
                    )));
                    // Show text content from tool results
                    let text = result.get_text();
                    if !text.is_empty() {
                        for line in text.lines() {
                            all_lines.push(Line::from(Span::styled(
                                line.to_string(),
                                Style::default().fg(Color::DarkGray),
                            )));
                        }
                    }
                }
            }
        }

        // Add blank line between messages
        all_lines.push(Line::from(""));
    }

    // Add current streaming response if active
    if app.is_streaming && !app.current_response.is_empty() {
        all_lines.push(Line::from(Span::styled(
            format!("[{}]", app.engine.get_model_name()),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        )));
        for line in app.current_response.lines() {
            all_lines.push(Line::from(line.to_string()));
        }
        all_lines.push(Line::from(""));
    }

    // Calculate scroll position
    // scroll_offset=0 means auto-scroll to bottom, higher values scroll up from bottom
    let total_lines = all_lines.len();
    let visible_height = chunks[0].height.saturating_sub(2) as usize; // -2 for borders
    let max_scroll = total_lines.saturating_sub(visible_height);

    // Clamp scroll_offset to valid range
    if app.scroll_offset > max_scroll {
        app.scroll_offset = max_scroll;
    }
    let clamped_offset = app.scroll_offset;

    // Calculate actual scroll: start from bottom, then apply manual offset (scrolling up)
    let effective_scroll = max_scroll.saturating_sub(clamped_offset);

    let chat_content = Paragraph::new(all_lines)
        .block(Block::default().borders(Borders::ALL).title("Chat"))
        .scroll((effective_scroll as u16, 0));

    f.render_widget(chat_content, chunks[0]);

    // Render scrollbar
    if total_lines > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"));

        // ScrollbarState position is from top, so we use effective_scroll
        let mut scrollbar_state = ScrollbarState::new(max_scroll)
            .position(effective_scroll);

        // Render scrollbar in the inner area (inside the border)
        let scrollbar_area = chunks[0].inner(ratatui::layout::Margin { vertical: 1, horizontal: 0 });
        f.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
    }

    // Render input box
    let input_widget = Paragraph::new(app.input.value())
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title("Message (/ for commands)"));

    f.render_widget(input_widget, chunks[1]);

    // Render status bar
    let message_count = app.engine.get_session().try_lock().map(|s| s.len()).unwrap_or(0);
    
    let is_listening = app.voice_coordinator.as_ref().map(|v| v.is_listening()).unwrap_or(false);
    let is_transcribing = app.voice_coordinator.as_ref().map(|v| v.is_transcribing()).unwrap_or(false);

    let voice_indicator = if is_listening {
        " | 🎤 Listening..."
    } else if app.voice_coordinator.is_some() {
        " | 🎙️ Voice"
    } else {
        ""
    };
    let status_text = if let Some(ref msg) = app.status_message {
        format!(" {} • {} | {}{} ", app.current_provider, app.engine.get_model_name(), msg, voice_indicator)
    } else if completion_loading {
        format!(" {} • {} | {} Loading completions...{} ", app.current_provider, app.engine.get_model_name(), app.get_thinking_indicator(), voice_indicator)
    } else if is_transcribing {
        format!(" {} • {} | {} Transcribing...{} ", app.current_provider, app.engine.get_model_name(), app.get_thinking_indicator(), voice_indicator)
    } else if app.is_streaming {
        format!(" {} • {} | {} Thinking...{} ", app.current_provider, app.engine.get_model_name(), app.get_thinking_indicator(), voice_indicator)
    } else {
        format!(" {} • {} | {} messages{} ", app.current_provider, app.engine.get_model_name(), message_count, voice_indicator)
    };

    let status_bar = Paragraph::new(status_text)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));

    f.render_widget(status_bar, chunks[2]);

    // Render completion popup if showing
    if let Some((completions, selected)) = completion_data {
        let completion_items: Vec<ListItem> = completions
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let label = c.label.as_ref().unwrap_or(&c.value);
                let desc = c.description.as_ref().map(|d| format!(" - {}", d)).unwrap_or_default();
                let text = format!("{}{}", label, desc);

                let style = if i == selected {
                    Style::default().bg(Color::Blue).fg(Color::White)
                } else {
                    Style::default()
                };

                ListItem::new(text).style(style)
            })
            .collect();

        let completion_list = List::new(completion_items)
            .block(Block::default().borders(Borders::ALL).title("Completions"));

        // Position popup above input box
        let popup_height = (completions.len() as u16 + 2).min(10);
        let popup_y = chunks[1].y.saturating_sub(popup_height);
        let popup_area = ratatui::layout::Rect {
            x: chunks[1].x,
            y: popup_y,
            width: chunks[1].width,
            height: popup_height,
        };

        f.render_widget(completion_list, popup_area);
    }

    // Set cursor position in input box
    f.set_cursor_position((
        chunks[1].x + app.input.visual_cursor() as u16 + 1,
        chunks[1].y + 1,
    ));
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    let whisper_model_path = args.get_whisper_model_path();

    // Setup file-based logging
    // In dev mode, use local ./noema.log that gets recreated on each run
    // In release mode, use Application Support directory with daily rotation
    #[cfg(debug_assertions)]
    let log_file = {
        let path = PathBuf::from("./noema.log");
        // Truncate existing log file
        let _ = std::fs::remove_file(&path);
        std::fs::File::create(&path)?
    };
    #[cfg(debug_assertions)]
    let (non_blocking, _guard) = tracing_appender::non_blocking(log_file);

    #[cfg(not(debug_assertions))]
    let (non_blocking, _guard) = {
        let log_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("noema")
            .join("logs");
        std::fs::create_dir_all(&log_dir)?;
        let file_appender = RollingFileAppender::new(Rotation::DAILY, &log_dir, "noema.log");
        tracing_appender::non_blocking(file_appender)
    };

    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
        .init();

    tracing::info!("Starting noema TUI");

    // Load environment variables from .env files
    load_env_file();
    let provider_urls = ProviderUrls::from_env();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and start
    let mut app = AppWithCommands::new(ModelProviderType::Gemini, None, provider_urls, whisper_model_path)?;
    app.start();

    let mut should_quit = false;

    while !should_quit {
        // Render UI
        terminal.draw(|f| ui(f, &mut app))?;

        // Update app state
        app.command_handler.target_mut().check_engine_events();
        app.command_handler.target_mut().check_voice_events();
        
        let is_listening = app.command_handler.target().voice_coordinator.as_ref().map(|v| v.is_listening()).unwrap_or(false);
        let is_transcribing = app.command_handler.target().voice_coordinator.as_ref().map(|v| v.is_transcribing()).unwrap_or(false);

        if app.command_handler.target().is_streaming || is_listening || is_transcribing {
            app.command_handler.target_mut().advance_thinking_animation();
        }

        // Handle input
        if event::poll(std::time::Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    should_quit = !app.handle_key_event(key).await?;
                }
                Event::Mouse(mouse) => {
                    match mouse.kind {
                        MouseEventKind::ScrollUp => {
                            app.command_handler.target_mut().scroll_up(3);
                        }
                        MouseEventKind::ScrollDown => {
                            app.command_handler.target_mut().scroll_down(3);
                        }
                        _ => {} // Ignore other mouse events
                    }
                }
                _ => {} // Ignore other events
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    Ok(())
}
