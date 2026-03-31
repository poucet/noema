//! In-process implementation of the daemon API traits.
//!
//! `EmbeddedDaemon` hosts the full daemon logic inside the calling process —
//! no networking, no separate binary. This is the first (and simplest)
//! implementation and the one Noema uses until the WebSocket layer is built.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc, Mutex};

use llm::ChatModel;

use simply_core::storage::coordinator::StorageCoordinator;
use simply_core::storage::ids::{AssetId, ConversationId, UserId};
use simply_core::storage::session::{ResolvedMessage, Session};
use simply_core::storage::traits::{StorageTypes, Stores};
use simply_core::storage::DocumentResolver;
use simply_core::{ConversationManager, ManagerEvent, McpRegistry, SharedEventSender};

use crate::api::*;
use crate::mcp::{DaemonMcpServer, ServerHandle, start_server};

// ---------------------------------------------------------------------------
// Session bookkeeping
// ---------------------------------------------------------------------------

struct ManagedSession<S: StorageTypes> {
    info: SessionInfo,
    manager: ConversationManager<S>,
    /// Per-session broadcast sender — subscribers get a receiver from this.
    event_broadcast: broadcast::Sender<DaemonEvent>,
}

// ---------------------------------------------------------------------------
// EmbeddedDaemon
// ---------------------------------------------------------------------------

/// In-process daemon — all operations are direct Rust calls, no networking.
pub struct EmbeddedDaemon<S: StorageTypes> {
    coordinator: Arc<StorageCoordinator<S>>,
    stores: Arc<dyn Stores<S>>,
    mcp_registry: Arc<Mutex<McpRegistry>>,
    sessions: Mutex<HashMap<SessionId, ManagedSession<S>>>,
    model: Mutex<Arc<dyn ChatModel + Send + Sync>>,
    model_id: Mutex<String>,
    user_id: UserId,
    /// Shared channel that all ConversationManagers send to.
    /// A dispatcher task routes events to per-session broadcast senders.
    manager_event_tx: SharedEventSender,
    /// Handle to the embedded MCP server (daemon tools like spawn_agent).
    _mcp_server_handle: Mutex<Option<ServerHandle>>,
}

impl<S: StorageTypes> EmbeddedDaemon<S>
where
    S::Document: DocumentResolver,
{
    pub async fn new(
        coordinator: Arc<StorageCoordinator<S>>,
        stores: Arc<dyn Stores<S>>,
        mcp_registry: Arc<Mutex<McpRegistry>>,
        model: Arc<dyn ChatModel + Send + Sync>,
        model_id: String,
        user_id: UserId,
    ) -> anyhow::Result<Arc<Self>> {
        let (manager_event_tx, manager_event_rx) = mpsc::unbounded_channel();

        // Start the daemon's MCP server (exposes tools like spawn_agent)
        let document_resolver: Arc<dyn DocumentResolver> = stores.document();
        let mcp_server = DaemonMcpServer::new(
            Arc::clone(&coordinator),
            Arc::clone(&mcp_registry),
            document_resolver,
        );
        let server_handle = start_server(mcp_server).await?;
        let server_url = server_handle.url();

        // Register and connect the MCP server in the registry
        {
            let mut registry = mcp_registry.lock().await;
            registry.register_ephemeral("daemon-tools".to_string(), server_url);
            registry.connect("daemon-tools").await?;
        }

        let daemon = Arc::new(Self {
            coordinator,
            stores,
            mcp_registry,
            sessions: Mutex::new(HashMap::new()),
            model: Mutex::new(model),
            model_id: Mutex::new(model_id),
            user_id,
            manager_event_tx,
            _mcp_server_handle: Mutex::new(Some(server_handle)),
        });

        Self::spawn_event_dispatcher(Arc::clone(&daemon), manager_event_rx);

        Ok(daemon)
    }

    /// Background task: receives (ConversationId, ManagerEvent) from all managers
    /// and forwards to the appropriate per-session broadcast channel.
    fn spawn_event_dispatcher(
        daemon: Arc<Self>,
        mut rx: mpsc::UnboundedReceiver<(ConversationId, ManagerEvent)>,
    ) {
        tokio::spawn(async move {
            while let Some((conversation_id, event)) = rx.recv().await {
                let session_id = SessionId::new(conversation_id.as_str());
                let sessions = daemon.sessions.lock().await;
                if let Some(managed) = sessions.get(&session_id) {
                    // Convert ManagerEvent → DaemonEvent and broadcast
                    let daemon_events = Self::manager_event_to_daemon_events(&session_id, event);
                    for daemon_event in daemon_events {
                        // Ignore send errors (no active subscribers)
                        let _ = managed.event_broadcast.send(daemon_event);
                    }
                }
            }
        });
    }

    fn manager_event_to_daemon_events(
        session_id: &SessionId,
        event: ManagerEvent,
    ) -> Vec<DaemonEvent> {
        match event {
            ManagerEvent::UserMessageAdded(_) => vec![],
            ManagerEvent::StreamingMessage(msg) => {
                msg.payload.content.into_iter().map(DaemonEvent::AssistantContent).collect()
            }
            ManagerEvent::Complete(_) => vec![DaemonEvent::TurnComplete],
            ManagerEvent::Error(err) => vec![DaemonEvent::Error(err)],
            ManagerEvent::ModelChanged(_) => vec![],
            ManagerEvent::Truncated(_) => vec![],
        }
    }

    fn build_manager(
        &self,
        session: Session<S>,
        model: Arc<dyn ChatModel + Send + Sync>,
        model_id: String,
    ) -> ConversationManager<S> {
        let document_resolver: Arc<dyn DocumentResolver> = self.stores.document();
        ConversationManager::new(
            session,
            Arc::clone(&self.coordinator),
            model,
            model_id,
            Arc::clone(&self.mcp_registry),
            document_resolver,
            self.user_id.clone(),
            self.manager_event_tx.clone(),
        )
    }

    fn make_session_info(
        session_id: &SessionId,
        persistence: Persistence,
        model_id: String,
    ) -> SessionInfo {
        SessionInfo {
            id: session_id.clone(),
            persistence,
            model_id,
            created_at: {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                format!("{}", now.as_secs())
            },
        }
    }
}

// ---------------------------------------------------------------------------
// SessionApi
// ---------------------------------------------------------------------------

#[async_trait]
impl<S: StorageTypes> SessionApi for EmbeddedDaemon<S>
where
    S::Document: DocumentResolver,
{
    async fn create_session(
        &self,
        options: CreateSessionOptions,
    ) -> anyhow::Result<(SessionInfo, broadcast::Receiver<DaemonEvent>)> {
        let conversation_id = ConversationId::new();
        let session_id = SessionId::new(conversation_id.as_str());
        let persistence = options.persistence.unwrap_or(Persistence::Persistent);

        let model = self.model.lock().await.clone();
        let model_id = match options.model_id {
            Some(id) => id,
            None => self.model_id.lock().await.clone(),
        };

        let session = Session::new(Arc::clone(&self.coordinator), conversation_id);
        let manager = self.build_manager(session, model, model_id.clone());
        let info = Self::make_session_info(&session_id, persistence, model_id);

        let (broadcast_tx, broadcast_rx) = broadcast::channel(256);

        self.sessions.lock().await.insert(
            session_id.clone(),
            ManagedSession {
                info: info.clone(),
                manager,
                event_broadcast: broadcast_tx,
            },
        );

        tracing::info!(session_id = %session_id, "session created");
        Ok((info, broadcast_rx))
    }

    async fn resume_session(
        &self,
        session_id: &SessionId,
    ) -> anyhow::Result<(SessionInfo, broadcast::Receiver<DaemonEvent>)> {
        // If already loaded, return info + new subscriber
        {
            let sessions = self.sessions.lock().await;
            if let Some(managed) = sessions.get(session_id) {
                return Ok((managed.info.clone(), managed.event_broadcast.subscribe()));
            }
        }

        // Load from storage
        let conversation_id = ConversationId::from_string(session_id.as_str());
        let session = Session::open(Arc::clone(&self.coordinator), conversation_id).await?;

        let model = self.model.lock().await.clone();
        let model_id = self.model_id.lock().await.clone();
        let manager = self.build_manager(session, model, model_id.clone());
        let info = Self::make_session_info(session_id, Persistence::Persistent, model_id);

        let (broadcast_tx, broadcast_rx) = broadcast::channel(256);

        self.sessions.lock().await.insert(
            session_id.clone(),
            ManagedSession {
                info: info.clone(),
                manager,
                event_broadcast: broadcast_tx,
            },
        );

        tracing::info!(session_id = %session_id, "session resumed from storage");
        Ok((info, broadcast_rx))
    }

    async fn subscribe_session(
        &self,
        session_id: &SessionId,
    ) -> anyhow::Result<broadcast::Receiver<DaemonEvent>> {
        let sessions = self.sessions.lock().await;
        let managed = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;
        Ok(managed.event_broadcast.subscribe())
    }

    async fn close_session(&self, session_id: &SessionId) -> anyhow::Result<()> {
        let removed = self.sessions.lock().await.remove(session_id);
        if removed.is_some() {
            tracing::info!(session_id = %session_id, "session closed");
            Ok(())
        } else {
            anyhow::bail!("session not found: {session_id}")
        }
    }

    async fn close_all_sessions(&self) -> anyhow::Result<()> {
        let mut sessions = self.sessions.lock().await;
        let count = sessions.len();
        sessions.clear();
        tracing::info!(count, "all sessions closed");
        Ok(())
    }

    async fn seed_context(
        &self,
        _session_id: &SessionId,
        _messages: Vec<SeedMessage>,
    ) -> anyhow::Result<()> {
        tracing::warn!("seed_context not yet implemented");
        Ok(())
    }

    async fn list_sessions(&self) -> anyhow::Result<Vec<SessionInfo>> {
        let sessions = self.sessions.lock().await;
        Ok(sessions.values().map(|s| s.info.clone()).collect())
    }

    async fn set_persistence(
        &self,
        session_id: &SessionId,
        persistence: Persistence,
    ) -> anyhow::Result<()> {
        let mut sessions = self.sessions.lock().await;
        let managed = sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;
        managed.info.persistence = persistence;
        Ok(())
    }

    async fn send_message(
        &self,
        session_id: &SessionId,
        message: UserMessage,
    ) -> anyhow::Result<()> {
        let sessions = self.sessions.lock().await;
        let managed = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;

        let tool_config = message
            .tool_filter
            .map(|f| f.into_tool_config())
            .unwrap_or_else(simply_core::ToolConfig::all_enabled);

        managed.manager.send_message(message.content, tool_config);
        Ok(())
    }

    async fn get_messages(
        &self,
        session_id: &SessionId,
    ) -> anyhow::Result<Vec<ResolvedMessage>> {
        let sessions = self.sessions.lock().await;
        let managed = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;
        Ok(managed.manager.messages_for_display().await)
    }

    async fn set_model(&self, session_id: &SessionId, model_id: &str) -> anyhow::Result<()> {
        let new_model = llm::create_model(model_id)?;
        let mut sessions = self.sessions.lock().await;
        let managed = sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;
        managed.manager.set_model(new_model, model_id.to_string());
        managed.info.model_id = model_id.to_string();
        Ok(())
    }

    async fn reload(&self, session_id: &SessionId) -> anyhow::Result<()> {
        let sessions = self.sessions.lock().await;
        let managed = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;
        managed.manager.reload().await?;
        Ok(())
    }

    async fn push_event(&self, event: InboundEvent) -> anyhow::Result<()> {
        tracing::info!(event_type = %event.event_type, "inbound event received (not yet routed)");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// AssetApi
// ---------------------------------------------------------------------------

#[async_trait]
impl<S: StorageTypes> AssetApi for EmbeddedDaemon<S>
where
    S::Document: DocumentResolver,
{
    async fn store_asset(&self, data: Vec<u8>, media_type: &str) -> anyhow::Result<AssetId> {
        use base64::{engine::general_purpose::STANDARD, Engine};
        let b64 = STANDARD.encode(&data);
        self.coordinator.store_asset(&b64, media_type).await
    }
}

// ---------------------------------------------------------------------------
// McpApi
// ---------------------------------------------------------------------------

#[async_trait]
impl<S: StorageTypes> McpApi for EmbeddedDaemon<S>
where
    S::Document: DocumentResolver,
{
    async fn register_mcp(&self, registration: McpRegistration) -> anyhow::Result<()> {
        let config = simply_core::ServerConfig {
            name: registration.name.clone(),
            url: registration.endpoint,
            auth: simply_core::AuthMethod::None,
            auth_token: None,
            auto_connect: true,
            auto_retry: false,
            use_well_known: false,
        };
        let mut registry = self.mcp_registry.lock().await;
        registry.add_server(registration.name.clone(), config);
        registry.connect(&registration.name).await?;
        tracing::info!(name = %registration.name, "MCP service registered and connected");
        Ok(())
    }

    async fn unregister_mcp(&self, name: &str) -> anyhow::Result<()> {
        let mut registry = self.mcp_registry.lock().await;
        registry.disconnect(name).await?;
        tracing::info!(name = %name, "MCP service unregistered");
        Ok(())
    }

    async fn list_tools(&self) -> anyhow::Result<Vec<String>> {
        let registry = self.mcp_registry.lock().await;
        Ok(registry
            .list_servers()
            .iter()
            .map(|(id, _)| id.to_string())
            .collect())
    }
}

// ---------------------------------------------------------------------------
// ModelApi
// ---------------------------------------------------------------------------

#[async_trait]
impl<S: StorageTypes> ModelApi for EmbeddedDaemon<S>
where
    S::Document: DocumentResolver,
{
    async fn list_models(&self) -> anyhow::Result<Vec<llm::ModelInfo>> {
        let mut all_models = Vec::new();
        for (_provider_name, result) in llm::list_all_models().await {
            if let Ok(models) = result {
                all_models.extend(models);
            }
        }
        Ok(all_models)
    }

    async fn default_model_id(&self) -> String {
        self.model_id.lock().await.clone()
    }

    async fn set_default_model(&self, model_id: &str) -> anyhow::Result<()> {
        let new_model = llm::create_model(model_id)?;
        *self.model.lock().await = new_model;
        *self.model_id.lock().await = model_id.to_string();
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// VoiceApi
// ---------------------------------------------------------------------------

#[async_trait]
impl<S: StorageTypes> VoiceApi for EmbeddedDaemon<S>
where
    S::Document: DocumentResolver,
{
    async fn voice_connect(&self, _session_id: &SessionId) -> anyhow::Result<VoiceHandle> {
        let (audio_tx, _audio_rx) = mpsc::channel(32);
        let (_voice_tx, voice_rx) = mpsc::channel(32);
        Ok(VoiceHandle {
            audio_in: audio_tx,
            events: voice_rx,
        })
    }
}
