use crate::{Agent, ConversationContext, McpAgent, McpRegistry, McpToolRegistry};
use crate::storage::document::resolver::DocumentResolver;
use crate::storage::session::{SessionStore, StorageTransaction};
use llm::{create_model, ChatMessage, ChatModel, ChatPayload};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use futures::future::join_all;

/// Configuration for which tools to enable during a request.
/// This allows fine-grained control over MCP tool availability.
#[derive(Debug, Clone, Default)]
pub struct ToolConfig {
    /// Whether tools are enabled at all. If false, no tools will be included.
    pub enabled: bool,
    /// Optional list of specific server IDs to include. If None, all servers are included.
    pub server_ids: Option<Vec<String>>,
    /// Optional list of specific tool names to include. If None, all tools are included.
    pub tool_names: Option<Vec<String>>,
}

impl ToolConfig {
    /// Create a config with all tools enabled
    pub fn all_enabled() -> Self {
        Self { enabled: true, server_ids: None, tool_names: None }
    }

    /// Create a config with all tools disabled
    pub fn disabled() -> Self {
        Self { enabled: false, server_ids: None, tool_names: None }
    }
}

pub enum EngineCommand {
    /// Send a message with tool configuration
    SendMessage(ChatPayload, ToolConfig),
    /// Send a message to multiple models in parallel
    /// (payload, model_ids) where model_ids are full IDs like "anthropic/claude-sonnet-4-5"
    SendParallelMessage(ChatPayload, Vec<String>),
    SetModel(Arc<dyn ChatModel + Send + Sync>),
    ClearHistory,
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// A message chunk (can contain text, images, audio, tool calls, etc.)
    Message(ChatMessage),
    MessageComplete,
    Error(String),
    ModelChanged(String),
    HistoryCleared,

    // Parallel execution events
    /// Streaming message from a specific model during parallel execution
    ParallelStreamingMessage {
        model_id: String,
        message: ChatMessage,
    },
    /// A model completed its response during parallel execution
    ParallelModelComplete {
        model_id: String,
        messages: Vec<ChatMessage>,
    },
    /// All parallel models have completed
    ParallelComplete {
        /// The span set ID for this parallel response group
        span_set_id: String,
        /// Info about each model's response for display
        alternates: Vec<ParallelAlternateInfo>,
    },
    /// Error from a specific model during parallel execution
    ParallelModelError {
        model_id: String,
        error: String,
    },
}

/// Information about a parallel model response (for UI display)
#[derive(Debug, Clone)]
pub struct ParallelAlternateInfo {
    pub span_id: String,
    pub model_id: String,
    pub model_display_name: String,
    pub message_count: usize,
    pub is_selected: bool,
}

/// Chat engine that manages conversation sessions with any storage backend
///
/// Generic over `S: SessionStore` to support both in-memory and persistent storage.
pub struct ChatEngine<S: SessionStore + 'static> {
    session: Arc<Mutex<S>>,
    mcp_registry: Arc<Mutex<McpRegistry>>,
    cmd_tx: mpsc::UnboundedSender<EngineCommand>,
    event_rx: mpsc::UnboundedReceiver<EngineEvent>,
    model: Arc<dyn ChatModel + Send + Sync>,
    #[allow(dead_code)]
    processor_handle: JoinHandle<()>,
    #[allow(dead_code)]
    document_resolver: Arc<dyn DocumentResolver>,
}

impl<S: SessionStore + 'static> ChatEngine<S>
where
    S::Transaction: Send + 'static,
{
    /// Create a new chat engine
    ///
    /// # Arguments
    /// * `session` - The session store for conversation history
    /// * `model` - The LLM model to use
    /// * `mcp_registry` - Registry of MCP servers for tool access
    /// * `document_resolver` - Resolver for document references (required for RAG)
    pub fn new(
        session: S,
        model: Arc<dyn ChatModel + Send + Sync>,
        mcp_registry: McpRegistry,
        document_resolver: Arc<dyn DocumentResolver>,
    ) -> Self {
        let session = Arc::new(Mutex::new(session));
        let mcp_registry = Arc::new(Mutex::new(mcp_registry));
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let session_clone = Arc::clone(&session);
        let mcp_registry_clone = Arc::clone(&mcp_registry);
        let initial_model = Arc::clone(&model);
        let resolver_clone = Arc::clone(&document_resolver);

        let processor_handle = tokio::spawn(async move {
            Self::processor_loop(
                session_clone,
                initial_model,
                mcp_registry_clone,
                resolver_clone,
                cmd_rx,
                event_tx,
            )
            .await;
        });

        Self {
            session,
            mcp_registry,
            cmd_tx,
            event_rx,
            model,
            processor_handle,
            document_resolver,
        }
    }

    async fn processor_loop(
        session: Arc<Mutex<S>>,
        mut model: Arc<dyn ChatModel + Send + Sync>,
        mcp_registry: Arc<Mutex<McpRegistry>>,
        document_resolver: Arc<dyn DocumentResolver>,
        mut cmd_rx: mpsc::UnboundedReceiver<EngineCommand>,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
    ) {
        // Create dynamic tool registry that queries MCP servers on each call
        let tool_registry = McpToolRegistry::new(Arc::clone(&mcp_registry));
        // Agent with document resolver for RAG support
        let agent = McpAgent::new(Arc::new(tool_registry), 10, Arc::clone(&document_resolver));

        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                EngineCommand::SendMessage(payload, tool_config) => {
                    // 1. Begin transaction and add user message to it
                    let mut tx = {
                        let sess = session.lock().await;
                        let mut tx = sess.begin();
                        // Add user message to transaction (will be written to DB on commit)
                        tx.add(ChatMessage::user(payload));
                        tx
                    };

                    // 2. Run Agent (streaming) WITHOUT holding Session lock
                    // If tools are disabled, use agent without tools
                    let execute_result = if tool_config.enabled {
                        agent.execute_stream(&mut tx, model.clone()).await
                    } else {
                        // Create a no-tools agent for this request
                        agent.execute_stream_no_tools(&mut tx, model.clone()).await
                    };
                    match execute_result {
                        Ok(_) => {
                            // 3. Send pending messages (full multimodal content) to UI
                            let pending = tx.pending();
                            for msg in pending.iter() {
                                let _ = event_tx.send(EngineEvent::Message(msg.clone()));
                            }

                            // 4. Commit transaction
                            let mut sess = session.lock().await;
                            match sess.commit(tx).await {
                                Ok(_) => {
                                    let _ = event_tx.send(EngineEvent::MessageComplete);
                                }
                                Err(e) => {
                                    let _ = event_tx
                                        .send(EngineEvent::Error(format!("Commit failed: {}", e)));
                                }
                            }
                        }
                        Err(e) => {
                            let _ = event_tx.send(EngineEvent::Error(e.to_string()));
                        }
                    }
                }
                EngineCommand::SendParallelMessage(payload, model_ids) => {
                    // 1. Add user message to session first
                    let user_msg = ChatMessage::user(payload.clone());
                    {
                        let mut sess = session.lock().await;
                        let mut tx = sess.begin();
                        tx.add(user_msg.clone());
                        if let Err(e) = sess.commit(tx).await {
                            let _ = event_tx.send(EngineEvent::Error(format!("Failed to save user message: {}", e)));
                            continue;
                        }
                    }

                    // 2. Get conversation history for context
                    let history: Vec<ChatMessage> = {
                        let sess = session.lock().await;
                        sess.messages().to_vec()
                    };

                    // 3. Spawn parallel tasks for each model
                    let mut handles = Vec::new();

                    for model_id in model_ids {
                        let event_tx = event_tx.clone();
                        let history = history.clone();
                        let mcp_registry = Arc::clone(&mcp_registry);
                        let resolver = Arc::clone(&document_resolver);

                        let handle = tokio::spawn(async move {
                            // Create model for this parallel request
                            let model = match create_model(&model_id) {
                                Ok(m) => m,
                                Err(e) => {
                                    let _ = event_tx.send(EngineEvent::ParallelModelError {
                                        model_id: model_id.clone(),
                                        error: format!("Failed to create model: {}", e),
                                    });
                                    return (model_id, Err(e.to_string()));
                                }
                            };

                            // Create a temporary in-memory context for this model's execution
                            let tool_registry = McpToolRegistry::new(Arc::clone(&mcp_registry));
                            let agent = McpAgent::new(Arc::new(tool_registry), 10, resolver);

                            // Create a minimal transaction with history
                            let mut messages = history.clone();

                            // Run agent with streaming
                            // For now, we use a simple approach: run the model and collect results
                            // In a future iteration, we could stream individual chunks
                            match run_single_model_agent(&agent, &mut messages, model, &event_tx, &model_id).await {
                                Ok(response_messages) => {
                                    // Send completion event for this model
                                    let _ = event_tx.send(EngineEvent::ParallelModelComplete {
                                        model_id: model_id.clone(),
                                        messages: response_messages.clone(),
                                    });
                                    (model_id, Ok(response_messages))
                                }
                                Err(e) => {
                                    let _ = event_tx.send(EngineEvent::ParallelModelError {
                                        model_id: model_id.clone(),
                                        error: e.clone(),
                                    });
                                    (model_id, Err(e))
                                }
                            }
                        });

                        handles.push(handle);
                    }

                    // 4. Wait for all models to complete
                    let results = join_all(handles).await;

                    // 5. Collect successful responses for parallel save
                    let mut successful_responses: Vec<(String, Vec<ChatMessage>)> = Vec::new();

                    for result in results {
                        if let Ok((model_id, Ok(messages))) = result {
                            successful_responses.push((model_id, messages));
                        }
                    }

                    // 6. Commit ALL responses as parallel alternates and get span info
                    // This saves each model's response as a separate span in the same span_set
                    let mut span_set_id = String::new();
                    let mut span_ids: Vec<String> = Vec::new();

                    if !successful_responses.is_empty() {
                        let mut sess = session.lock().await;
                        match sess.commit_parallel_responses(&successful_responses, 0).await {
                            Ok((set_id, ids)) => {
                                span_set_id = set_id;
                                span_ids = ids;
                            }
                            Err(e) => {
                                let _ = event_tx.send(EngineEvent::Error(format!("Failed to save parallel responses: {}", e)));
                            }
                        }
                    }

                    // 7. Build alternates with span info
                    let alternates: Vec<ParallelAlternateInfo> = successful_responses
                        .iter()
                        .enumerate()
                        .map(|(idx, (model_id, messages))| {
                            let display_name = model_id.split('/').last().unwrap_or(model_id).to_string();
                            let span_id = span_ids.get(idx).cloned().unwrap_or_default();
                            ParallelAlternateInfo {
                                span_id,
                                model_id: model_id.clone(),
                                model_display_name: display_name,
                                message_count: messages.len(),
                                is_selected: idx == 0,
                            }
                        })
                        .collect();

                    // 8. Send parallel complete event with span info
                    let _ = event_tx.send(EngineEvent::ParallelComplete { span_set_id, alternates });
                }
                EngineCommand::SetModel(new_model) => {
                    let name = new_model.name().to_string();
                    model = new_model;
                    let _ = event_tx.send(EngineEvent::ModelChanged(name));
                }
                EngineCommand::ClearHistory => {
                    let mut sess = session.lock().await;
                    let _ = sess.clear().await;
                    let _ = event_tx.send(EngineEvent::HistoryCleared);
                }
            }
        }
    }

    pub fn send_message(&self, payload: impl Into<ChatPayload>, tool_config: ToolConfig) {
        let _ = self.cmd_tx.send(EngineCommand::SendMessage(payload.into(), tool_config));
    }

    pub fn set_model(&mut self, model: Arc<dyn ChatModel + Send + Sync>) {
        self.model = Arc::clone(&model);
        let _ = self.cmd_tx.send(EngineCommand::SetModel(model));
    }

    pub fn clear_history(&self) {
        let _ = self.cmd_tx.send(EngineCommand::ClearHistory);
    }

    pub fn try_recv(&mut self) -> Option<EngineEvent> {
        match self.event_rx.try_recv() {
            Ok(event) => Some(event),
            Err(_) => None,
        }
    }

    pub async fn next_event(&mut self) -> Option<EngineEvent> {
        self.event_rx.recv().await
    }

    pub fn get_session(&self) -> Arc<Mutex<S>> {
        Arc::clone(&self.session)
    }

    pub fn get_mcp_registry(&self) -> Arc<Mutex<McpRegistry>> {
        Arc::clone(&self.mcp_registry)
    }

    pub fn get_model_name(&self) -> &str {
        self.model.name()
    }

    /// Send a message to multiple models in parallel
    pub fn send_parallel_message(&self, payload: impl Into<ChatPayload>, model_ids: Vec<String>) {
        let _ = self.cmd_tx.send(EngineCommand::SendParallelMessage(payload.into(), model_ids));
    }
}

/// Helper function to run a single model's agent loop
/// This is extracted to allow parallel execution of multiple models
async fn run_single_model_agent(
    agent: &McpAgent,
    messages: &mut Vec<ChatMessage>,
    model: Arc<dyn ChatModel + Send + Sync>,
    event_tx: &mpsc::UnboundedSender<EngineEvent>,
    model_id: &str,
) -> Result<Vec<ChatMessage>, String> {
    use crate::storage::session::MemorySession;

    // Create a temporary in-memory session with the conversation history
    let mut temp_session = MemorySession::new();
    for msg in messages.iter() {
        let mut tx = temp_session.begin();
        tx.add(msg.clone());
        temp_session.commit(tx).await.map_err(|e| e.to_string())?;
    }

    // Begin a new transaction for the agent to add responses to
    let mut tx = temp_session.begin();

    // Run the agent
    match agent.execute_stream(&mut tx, model).await {
        Ok(_) => {
            // Get the pending messages (new responses from this model)
            let response_messages: Vec<ChatMessage> = tx.pending().to_vec();

            // Stream each message as it's generated
            for msg in &response_messages {
                let _ = event_tx.send(EngineEvent::ParallelStreamingMessage {
                    model_id: model_id.to_string(),
                    message: msg.clone(),
                });
            }

            // Explicitly rollback - we don't need to persist to the temp session
            // The real response will be committed by the main engine loop
            tx.rollback();

            Ok(response_messages)
        }
        Err(e) => {
            tx.rollback();
            Err(e.to_string())
        }
    }
}
