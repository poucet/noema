use anyhow::Result;
use conversation::Conversation;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use dotenv;
use futures::StreamExt;
use llm::providers::{ClaudeProvider, GeminiProvider, GeneralModelProvider, OllamaProvider, OpenAIProvider};
use llm::ModelProvider;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};
use std::io;
use std::str::FromStr;
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

fn get_api_key(key: &str) -> String {
    let home_dir = if let Some(home) = directories::UserDirs::new() {
        home.home_dir().to_path_buf()
    } else {
        panic!("Could not determine home directory");
    };
    let env_path = home_dir.join(".env");
    dotenv::from_path(env_path).ok();
    std::env::var(key).expect(&format!("{} must be set in .env file", key))
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

struct App {
    input: Input,
    conversation: Conversation,
    current_provider: ModelProviderType,
    model_display_name: &'static str,
    status_message: Option<String>,
    is_streaming: bool,
}

impl App {
    fn new(provider: ModelProviderType, system_message: Option<String>) -> Result<Self> {
        let (model_id, model_display_name) = get_model_info(&provider);
        let provider_instance = create_provider(&provider);
        let model = provider_instance.create_chat_model(model_id).unwrap();

        let conversation = if let Some(sys_msg) = system_message {
            Conversation::with_system_message(model, sys_msg)
        } else {
            Conversation::new(model)
        };

        Ok(App {
            input: Input::default(),
            conversation,
            current_provider: provider,
            model_display_name,
            status_message: None,
            is_streaming: false,
        })
    }

    fn handle_command(&mut self, input: &str) -> Result<bool> {
        let parts: Vec<&str> = input[1..].split_whitespace().collect();
        if parts.is_empty() {
            return Ok(true);
        }

        match parts[0] {
            "quit" | "exit" => return Ok(false),
            "help" => {
                self.status_message = Some("Commands: /quit /clear /model <provider> /help".to_string());
            }
            "clear" => {
                self.conversation.clear();
                self.status_message = Some("Conversation cleared".to_string());
            }
            "model" => {
                if parts.len() < 2 {
                    self.status_message = Some("Usage: /model <provider> (ollama, gemini, claude, openai)".to_string());
                } else {
                    match ModelProviderType::from_str(parts[1]) {
                        Ok(new_provider) => {
                            let (new_model_id, new_model_display) = get_model_info(&new_provider);
                            let new_provider_instance = create_provider(&new_provider);

                            match new_provider_instance.create_chat_model(new_model_id) {
                                Some(new_model) => {
                                    self.conversation.set_model(new_model);
                                    self.current_provider = new_provider;
                                    self.model_display_name = new_model_display;
                                    self.status_message = Some(format!("Switched to {} • {}", self.current_provider, self.model_display_name));
                                }
                                None => {
                                    self.status_message = Some(format!("Failed to create model for {}", new_provider));
                                }
                            }
                        }
                        Err(e) => {
                            self.status_message = Some(e);
                        }
                    }
                }
            }
            _ => {
                self.status_message = Some(format!("Unknown command: /{}. Type /help for help.", parts[0]));
            }
        }

        Ok(true)
    }

    async fn send_message(&mut self, message: String) -> Result<()> {
        self.is_streaming = true;
        let mut stream = self.conversation.send_stream(&message).await?;

        while let Some(_chunk) = stream.next().await {
            // Chunks are accumulated automatically
        }

        self.is_streaming = false;
        Ok(())
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),      // Chat area
            Constraint::Length(3),   // Input area
            Constraint::Length(1),   // Status bar
        ])
        .split(f.area());

    // Render chat messages
    let history = app.conversation.history();
    let messages: Vec<ListItem> = history
        .iter()
        .map(|msg| {
            let role = match msg.role {
                llm::Role::User => "You",
                llm::Role::Assistant => &app.model_display_name,
                llm::Role::System => "System",
            };

            let style = match msg.role {
                llm::Role::User => Style::default().fg(Color::Cyan),
                llm::Role::Assistant => Style::default().fg(Color::Green),
                llm::Role::System => Style::default().fg(Color::Yellow),
            };

            // Parse markdown and render
            let content_lines: Vec<Line> = msg.get_text()
                .lines()
                .map(|line| {
                    if line.starts_with("```") {
                        Line::from(Span::styled(line, Style::default().fg(Color::DarkGray)))
                    } else if line.starts_with("# ") {
                        Line::from(Span::styled(line, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)))
                    } else if line.starts_with("## ") {
                        Line::from(Span::styled(line, Style::default().fg(Color::Yellow)))
                    } else if line.starts_with("- ") || line.starts_with("* ") {
                        Line::from(Span::styled(line, Style::default().fg(Color::Cyan)))
                    } else if line.starts_with("`") && line.ends_with("`") {
                        Line::from(Span::styled(line, Style::default().fg(Color::Magenta)))
                    } else {
                        Line::from(line)
                    }
                })
                .collect();

            let mut text = Text::default();
            text.lines.push(Line::from(Span::styled(
                format!("[{}]", role),
                style.add_modifier(Modifier::BOLD),
            )));
            text.lines.extend(content_lines);
            text.lines.push(Line::from(""));

            ListItem::new(text)
        })
        .collect();

    let messages_list = List::new(messages)
        .block(Block::default().borders(Borders::ALL).title("Chat"));

    f.render_widget(messages_list, chunks[0]);

    // Render input box
    let input_widget = Paragraph::new(app.input.value())
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title("Message (/ for commands)"));

    f.render_widget(input_widget, chunks[1]);

    // Render status bar
    let status_text = if let Some(ref msg) = app.status_message {
        format!(" {} • {} | {} ", app.current_provider, app.model_display_name, msg)
    } else if app.is_streaming {
        format!(" {} • {} | Streaming... ", app.current_provider, app.model_display_name)
    } else {
        format!(" {} • {} | {} messages ", app.current_provider, app.model_display_name, app.conversation.message_count())
    };

    let status_bar = Paragraph::new(status_text)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));

    f.render_widget(status_bar, chunks[2]);

    // Set cursor position in input box
    f.set_cursor_position((
        chunks[1].x + app.input.visual_cursor() as u16 + 1,
        chunks[1].y + 1,
    ));
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(ModelProviderType::Gemini, None)?;
    let mut should_quit = false;

    while !should_quit {
        terminal.draw(|f| ui(f, &app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                            should_quit = true;
                        }
                        (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                            should_quit = true;
                        }
                        (KeyCode::Enter, _) => {
                            let input_text = app.input.value().to_string();
                            app.input.reset();
                            app.status_message = None;

                            if !input_text.is_empty() {
                                if input_text.starts_with('/') {
                                    match app.handle_command(&input_text) {
                                        Ok(continue_running) => {
                                            should_quit = !continue_running;
                                        }
                                        Err(e) => {
                                            app.status_message = Some(format!("Error: {}", e));
                                        }
                                    }
                                } else {
                                    // Send regular message
                                    if let Err(e) = app.send_message(input_text).await {
                                        app.status_message = Some(format!("Error: {}", e));
                                    }
                                }
                            }
                        }
                        _ => {
                            app.input.handle_event(&Event::Key(key));
                        }
                    }
                }
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
