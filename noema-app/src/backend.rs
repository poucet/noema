//! Async backend runner - handles database, model providers, and engine

use crossbeam_channel::{Receiver, Sender};
use std::path::PathBuf;

use config::{create_provider, get_model_info, ModelProviderType, ProviderUrls};
use llm::ModelProvider;
use noema_core::{ChatEngine, EngineEvent, McpRegistry, SessionStore, SqliteSession, SqliteStore};

use crate::events::{AppCommand, CoreEvent, DisplayMessage, ModelInfo};

/// Spawn the async backend in a separate thread
pub fn spawn_async_backend(
    cmd_rx: Receiver<AppCommand>,
    event_tx: Sender<CoreEvent>,
    provider_urls: ProviderUrls,
) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        rt.block_on(async move {
            run_backend(cmd_rx, event_tx, provider_urls).await;
        });
    });
}

async fn run_backend(
    cmd_rx: Receiver<AppCommand>,
    event_tx: Sender<CoreEvent>,
    provider_urls: ProviderUrls,
) {
    // Initialize database
    let db_path = get_db_path();
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let store = match SqliteStore::open(&db_path) {
        Ok(s) => s,
        Err(e) => {
            let _ = event_tx.send(CoreEvent::Error(format!("Failed to open database: {}", e)));
            return;
        }
    };

    let session = match store.create_conversation() {
        Ok(s) => s,
        Err(e) => {
            let _ = event_tx.send(CoreEvent::Error(format!("Failed to create session: {}", e)));
            return;
        }
    };

    // Create default model (Gemini)
    let provider_type = ModelProviderType::Gemini;
    let (model_id, model_display_name) = get_model_info(&provider_type);
    let provider_instance = create_provider(&provider_type, &provider_urls);
    let model = match provider_instance.create_chat_model(model_id) {
        Some(m) => m,
        None => {
            let _ = event_tx.send(CoreEvent::Error(format!(
                "Failed to create model: {}",
                model_id
            )));
            return;
        }
    };

    let _ = event_tx.send(CoreEvent::ModelChanged(model_display_name.to_string()));

    let mcp_registry =
        McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()));
    let mut engine: ChatEngine<SqliteSession> =
        ChatEngine::new(session, model, model_display_name.to_string(), mcp_registry);

    // Track current conversation ID
    let mut current_conversation_id = {
        let session_arc = engine.get_session();
        let id = if let Ok(sess) = session_arc.try_lock() {
            sess.conversation_id().to_string()
        } else {
            String::new()
        };
        id
    };

    // Load initial history
    {
        let session_arc = engine.get_session();
        if let Ok(sess) = session_arc.try_lock() {
            let messages: Vec<DisplayMessage> = sess
                .messages()
                .iter()
                .map(DisplayMessage::from_chat_message)
                .collect();
            let _ = event_tx.send(CoreEvent::HistoryLoaded(messages));
        };
    }

    // Send initial conversations list
    if let Ok(convos) = store.list_conversations() {
        let _ = event_tx.send(CoreEvent::ConversationsList(convos));
    }

    // Main command loop
    loop {
        // Check for commands (non-blocking)
        if let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                AppCommand::SendMessage(payload) => {
                    // Send user message event immediately so UI shows it
                    let display_msg = DisplayMessage::from_payload(&payload);
                    let _ = event_tx.send(CoreEvent::UserMessageSent(display_msg));

                    engine.send_message(payload);
                }
                AppCommand::ClearHistory => {
                    engine.clear_history();
                }
                AppCommand::SetModel { model_id, provider } => {
                    let provider_type = match provider.to_lowercase().as_str() {
                        "ollama" => ModelProviderType::Ollama,
                        "gemini" => ModelProviderType::Gemini,
                        "claude" => ModelProviderType::Claude,
                        "openai" => ModelProviderType::OpenAI,
                        _ => {
                            let _ = event_tx
                                .send(CoreEvent::Error(format!("Unknown provider: {}", provider)));
                            continue;
                        }
                    };
                    let provider_instance = create_provider(&provider_type, &provider_urls);
                    if let Some(new_model) = provider_instance.create_chat_model(&model_id) {
                        // Use a shorter display name
                        let display_name = model_id
                            .split('/')
                            .last()
                            .unwrap_or(&model_id)
                            .to_string();
                        engine.set_model(new_model, display_name.clone());
                        let _ = event_tx.send(CoreEvent::ModelChanged(display_name));
                    } else {
                        let _ = event_tx
                            .send(CoreEvent::Error(format!("Model not found: {}", model_id)));
                    }
                }
                AppCommand::ListModels => {
                    // Collect models from all providers
                    let mut all_models = Vec::new();

                    // Gemini
                    let gemini = create_provider(&ModelProviderType::Gemini, &provider_urls);
                    if let Ok(models) = gemini.list_models().await {
                        for m in models {
                            all_models.push(ModelInfo {
                                id: m.id.clone(),
                                display_name: m.name().to_string(),
                                provider: "gemini".to_string(),
                            });
                        }
                    }

                    // Claude
                    let claude = create_provider(&ModelProviderType::Claude, &provider_urls);
                    if let Ok(models) = claude.list_models().await {
                        for m in models {
                            all_models.push(ModelInfo {
                                id: m.id.clone(),
                                display_name: m.name().to_string(),
                                provider: "claude".to_string(),
                            });
                        }
                    }

                    // OpenAI
                    let openai = create_provider(&ModelProviderType::OpenAI, &provider_urls);
                    if let Ok(models) = openai.list_models().await {
                        for m in models {
                            all_models.push(ModelInfo {
                                id: m.id.clone(),
                                display_name: m.name().to_string(),
                                provider: "openai".to_string(),
                            });
                        }
                    }

                    // Ollama
                    let ollama = create_provider(&ModelProviderType::Ollama, &provider_urls);
                    if let Ok(models) = ollama.list_models().await {
                        for m in models {
                            all_models.push(ModelInfo {
                                id: m.id.clone(),
                                display_name: m.name().to_string(),
                                provider: "ollama".to_string(),
                            });
                        }
                    }

                    let _ = event_tx.send(CoreEvent::ModelsList(all_models));
                }
                AppCommand::RenameConversation { id, name } => {
                    let name_opt = if name.trim().is_empty() {
                        None
                    } else {
                        Some(name.as_str())
                    };
                    if let Err(e) = store.rename_conversation(&id, name_opt) {
                        let _ = event_tx
                            .send(CoreEvent::Error(format!("Failed to rename: {}", e)));
                    } else {
                        let _ = event_tx.send(CoreEvent::ConversationRenamed);
                        // Refresh conversations list
                        if let Ok(convos) = store.list_conversations() {
                            let _ = event_tx.send(CoreEvent::ConversationsList(convos));
                        }
                    }
                }
                AppCommand::ListConversations => {
                    if let Ok(convos) = store.list_conversations() {
                        let _ = event_tx.send(CoreEvent::ConversationsList(convos));
                    }
                }
                AppCommand::SwitchConversation(id) => {
                    match store.open_conversation(&id) {
                        Ok(session) => {
                            // Get current model info from engine
                            let model_name = engine.get_model_name().to_string();
                            let mcp_registry = McpRegistry::load()
                                .unwrap_or_else(|_| McpRegistry::new(Default::default()));

                            // Recreate provider to get model
                            // For now, just use Gemini as default when switching
                            let provider_type = ModelProviderType::Gemini;
                            let (model_id, _) = get_model_info(&provider_type);
                            let provider_instance = create_provider(&provider_type, &provider_urls);
                            if let Some(model) = provider_instance.create_chat_model(model_id) {
                                engine = ChatEngine::new(session, model, model_name, mcp_registry);
                                current_conversation_id = id.clone();

                                // Load messages for this conversation
                                let session_arc = engine.get_session();
                                if let Ok(sess) = session_arc.try_lock() {
                                    let messages: Vec<DisplayMessage> = sess
                                        .messages()
                                        .iter()
                                        .map(DisplayMessage::from_chat_message)
                                        .collect();
                                    let _ = event_tx.send(CoreEvent::HistoryLoaded(messages));
                                };
                                let _ = event_tx.send(CoreEvent::ConversationSwitched(id));
                            }
                        }
                        Err(e) => {
                            let _ = event_tx.send(CoreEvent::Error(format!(
                                "Failed to open conversation: {}",
                                e
                            )));
                        }
                    }
                }
                AppCommand::NewConversation => {
                    match store.create_conversation() {
                        Ok(session) => {
                            let model_name = engine.get_model_name().to_string();
                            let mcp_registry = McpRegistry::load()
                                .unwrap_or_else(|_| McpRegistry::new(Default::default()));

                            let provider_type = ModelProviderType::Gemini;
                            let (model_id, _) = get_model_info(&provider_type);
                            let provider_instance = create_provider(&provider_type, &provider_urls);
                            if let Some(model) = provider_instance.create_chat_model(model_id) {
                                let new_id = {
                                    // Get the new conversation ID before we move the session
                                    session.conversation_id().to_string()
                                };
                                engine = ChatEngine::new(session, model, model_name, mcp_registry);
                                current_conversation_id = new_id.clone();

                                let _ = event_tx.send(CoreEvent::HistoryLoaded(vec![]));
                                let _ = event_tx.send(CoreEvent::ConversationCreated(new_id));

                                // Refresh conversations list
                                if let Ok(convos) = store.list_conversations() {
                                    let _ = event_tx.send(CoreEvent::ConversationsList(convos));
                                }
                            }
                        }
                        Err(e) => {
                            let _ = event_tx.send(CoreEvent::Error(format!(
                                "Failed to create conversation: {}",
                                e
                            )));
                        }
                    }
                }
                AppCommand::DeleteConversation(id) => {
                    // Don't delete current conversation
                    if id == current_conversation_id {
                        let _ = event_tx
                            .send(CoreEvent::Error("Cannot delete current conversation".into()));
                        continue;
                    }
                    if let Err(e) = store.delete_conversation(&id) {
                        let _ = event_tx
                            .send(CoreEvent::Error(format!("Failed to delete: {}", e)));
                    } else {
                        // Refresh list
                        if let Ok(convos) = store.list_conversations() {
                            let _ = event_tx.send(CoreEvent::ConversationsList(convos));
                        }
                    }
                }
            }
        }

        // Check for engine events (non-blocking)
        while let Some(engine_event) = engine.try_recv() {
            match engine_event {
                EngineEvent::Message(msg) => {
                    let display_msg = DisplayMessage::from_chat_message(&msg);
                    let _ = event_tx.send(CoreEvent::StreamingMessage(display_msg));
                }
                EngineEvent::MessageComplete => {
                    // Reload messages from session after completion
                    let session_arc = engine.get_session();
                    if let Ok(sess) = session_arc.try_lock() {
                        let messages: Vec<DisplayMessage> = sess
                            .messages()
                            .iter()
                            .map(DisplayMessage::from_chat_message)
                            .collect();
                        let _ = event_tx.send(CoreEvent::HistoryLoaded(messages));
                    };
                    let _ = event_tx.send(CoreEvent::MessageComplete);

                    // Refresh conversations list (message count may have changed)
                    if let Ok(convos) = store.list_conversations() {
                        let _ = event_tx.send(CoreEvent::ConversationsList(convos));
                    }
                }
                EngineEvent::Error(err) => {
                    let _ = event_tx.send(CoreEvent::Error(err));
                }
                EngineEvent::ModelChanged(name) => {
                    let _ = event_tx.send(CoreEvent::ModelChanged(name));
                }
                EngineEvent::HistoryCleared => {
                    let _ = event_tx.send(CoreEvent::HistoryCleared);
                }
            }
        }

        // Small sleep to avoid busy-waiting
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
}

pub fn get_db_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("noema")
            .join("conversations.db")
    }

    #[cfg(target_os = "android")]
    {
        PathBuf::from("/data/data/com.noema.app/databases/conversations.db")
    }

    #[cfg(not(any(target_os = "macos", target_os = "android")))]
    {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("noema")
            .join("conversations.db")
    }
}

pub fn get_whisper_model_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("noema")
        .join("models")
        .join("ggml-base.en.bin")
}
