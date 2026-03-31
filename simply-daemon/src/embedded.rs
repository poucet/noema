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
    /// Default model ID — used when sessions don't specify one.
    default_model_id: Mutex<String>,
    user_id: UserId,
    manager_event_tx: SharedEventSender,
    _mcp_server_handle: Mutex<Option<ServerHandle>>,
}

impl<S: StorageTypes> EmbeddedDaemon<S>
where
    S::Document: DocumentResolver,
{
    pub async fn new<T: Stores<S> + 'static>(
        stores: Arc<T>,
    ) -> anyhow::Result<Arc<Self>>
    where
        S::User: simply_core::storage::traits::UserStore,
    {
        let coordinator = Arc::new(StorageCoordinator::from_stores(&*stores));
        let stores: Arc<dyn Stores<S>> = stores;
        let settings = config::Settings::load();

        // Resolve user
        let user_id = Self::resolve_user(&*stores, &settings).await?;

        // Resolve default model
        const FALLBACK_MODEL_ID: &str = "claude/models/claude-sonnet-4-5-20250929";
        let default_model_id = settings
            .default_model
            .unwrap_or_else(|| FALLBACK_MODEL_ID.to_string());

        // Initialize MCP registry
        let mcp_registry = Arc::new(Mutex::new(
            McpRegistry::load().unwrap_or_else(|_| McpRegistry::new(Default::default()))
        ));

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
            default_model_id: Mutex::new(default_model_id.to_string()),
            user_id,
            manager_event_tx,
            _mcp_server_handle: Mutex::new(Some(server_handle)),
        });

        Self::spawn_event_dispatcher(Arc::clone(&daemon), manager_event_rx);

        // Auto-connect configured MCP servers in background
        {
            let registry = Arc::clone(&daemon.mcp_registry);
            simply_core::mcp::start_auto_connect(registry, None).await;
        }

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
            ManagerEvent::UserMessageAdded(msg) => vec![DaemonEvent::UserMessage(msg)],
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

    async fn resolve_user(
        stores: &dyn Stores<S>,
        settings: &config::Settings,
    ) -> anyhow::Result<UserId>
    where
        S::User: simply_core::storage::traits::UserStore,
    {
        use simply_core::storage::traits::UserStore;
        let user_store = stores.user();

        let user = if let Some(ref email) = settings.user_email {
            user_store.get_or_create_user_by_email(email).await?
        } else {
            let users = user_store.list_users().await?;
            match users.len() {
                0 => user_store.get_or_create_default_user().await?,
                1 => users.into_iter().next().unwrap(),
                _ => {
                    let emails: Vec<String> = users.iter().map(|u| u.email.clone()).collect();
                    anyhow::bail!("MULTIPLE_USERS:{}", emails.join(","));
                }
            }
        };
        Ok(user.id)
    }

    /// Access the MCP registry (for MCP config commands).
    pub fn mcp_registry(&self) -> &Arc<Mutex<McpRegistry>> {
        &self.mcp_registry
    }

    /// Access stores (for features not yet in daemon API, e.g. gdocs).
    pub fn stores(&self) -> &Arc<dyn Stores<S>> {
        &self.stores
    }

    /// Access coordinator (for features not yet in daemon API, e.g. gdocs).
    pub fn coordinator(&self) -> &Arc<StorageCoordinator<S>> {
        &self.coordinator
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

        let model_id = match options.model_id {
            Some(id) => id,
            None => self.default_model_id.lock().await.clone(),
        };
        let model = llm::create_model(&model_id)?;

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

        let model_id = self.default_model_id.lock().await.clone();
        let model = llm::create_model(&model_id)?;
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
// ConversationApi
// ---------------------------------------------------------------------------

#[async_trait]
impl<S: StorageTypes> ConversationApi for EmbeddedDaemon<S>
where
    S::Document: DocumentResolver,
{
    async fn create_conversation(&self, name: Option<&str>) -> anyhow::Result<ConversationId> {
        self.coordinator
            .create_conversation(&self.user_id, name)
            .await
    }

    async fn list_conversations(&self) -> anyhow::Result<Vec<ConversationInfo>> {
        use simply_core::storage::{EntityStore, EntityType, TurnStore};

        let entities = self.stores
            .entity()
            .list_entities(&self.user_id, Some(&EntityType::conversation()))
            .await?;

        let mut result = Vec::with_capacity(entities.len());
        for entity in entities {
            let turn_count = self.stores
                .turn()
                .get_turn_count(&entity.id)
                .await
                .unwrap_or(0);
            result.push(ConversationInfo {
                id: entity.id.clone(),
                name: entity.name.clone(),
                message_count: turn_count,
                created_at: entity.created_at,
            });
        }
        Ok(result)
    }

    async fn delete_conversation(&self, conversation_id: &ConversationId) -> anyhow::Result<()> {
        use simply_core::storage::EntityStore;

        // Close session if open
        let session_id = SessionId::new(conversation_id.as_str());
        let _ = self.close_session(&session_id).await;

        self.stores
            .entity()
            .delete_entity(conversation_id)
            .await
    }

    async fn rename_conversation(&self, conversation_id: &ConversationId, name: &str) -> anyhow::Result<()> {
        use simply_core::storage::EntityStore;

        let mut entity = self.stores
            .entity()
            .get_entity(conversation_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("conversation not found: {conversation_id}"))?;

        entity.name = if name.trim().is_empty() { None } else { Some(name.to_string()) };

        self.stores
            .entity()
            .update_entity(conversation_id, &entity)
            .await
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

    async fn get_blob(&self, hash: &simply_core::storage::types::BlobHash) -> anyhow::Result<Vec<u8>> {
        self.coordinator.get_blob(hash).await
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
    async fn list_mcp_servers(&self) -> anyhow::Result<Vec<McpServerInfo>> {
        let registry = self.mcp_registry.lock().await;
        let mut servers = Vec::new();
        for (id, config) in registry.list_servers() {
            let is_connected = registry.is_connected(id);
            let tool_count = registry.get_connection(id)
                .map(|c| c.tools.len())
                .unwrap_or(0);
            let status = match registry.get_status(id) {
                simply_core::mcp::ServerStatus::Disconnected => "disconnected".to_string(),
                simply_core::mcp::ServerStatus::Connected => "connected".to_string(),
                simply_core::mcp::ServerStatus::Retrying { attempt } => format!("retrying:{}", attempt),
                simply_core::mcp::ServerStatus::RetryStopped { last_error } => format!("stopped:{}", last_error),
            };
            let auth_type = match &config.auth {
                simply_core::AuthMethod::None => "none",
                simply_core::AuthMethod::Token { .. } => "token",
                simply_core::AuthMethod::OAuth { .. } => "oauth",
            };
            servers.push(McpServerInfo {
                id: id.to_string(),
                name: config.name.clone(),
                url: config.url.clone(),
                auth_type: auth_type.to_string(),
                is_connected,
                tool_count,
                status,
            });
        }
        Ok(servers)
    }

    async fn add_mcp_server(&self, request: AddMcpServerRequest) -> anyhow::Result<()> {
        let auth = match request.auth_type.as_str() {
            "token" => simply_core::AuthMethod::Token {
                token: request.auth_token.unwrap_or_default(),
            },
            _ => simply_core::AuthMethod::None,
        };
        let config = simply_core::ServerConfig {
            name: request.name,
            url: request.url,
            auth,
            auth_token: None,
            auto_connect: true,
            auto_retry: true,
            use_well_known: false,
        };
        let mut registry = self.mcp_registry.lock().await;
        registry.add_server(request.id.clone(), config);
        registry.save_config()?;
        registry.connect(&request.id).await?;
        Ok(())
    }

    async fn remove_mcp_server(&self, server_id: &str) -> anyhow::Result<()> {
        let mut registry = self.mcp_registry.lock().await;
        registry.remove_server(server_id).await?;
        registry.save_config()?;
        Ok(())
    }

    async fn connect_mcp_server(&self, server_id: &str) -> anyhow::Result<usize> {
        let mut registry = self.mcp_registry.lock().await;
        registry.connect(server_id).await?;
        let tool_count = registry.get_connection(server_id)
            .map(|c| c.tools.len())
            .unwrap_or(0);
        Ok(tool_count)
    }

    async fn disconnect_mcp_server(&self, server_id: &str) -> anyhow::Result<()> {
        let mut registry = self.mcp_registry.lock().await;
        registry.disconnect(server_id).await?;
        Ok(())
    }

    async fn get_mcp_server_tools(&self, server_id: &str) -> anyhow::Result<Vec<McpToolInfo>> {
        let registry = self.mcp_registry.lock().await;
        let conn = registry.get_connection(server_id)
            .ok_or_else(|| anyhow::anyhow!("server not connected: {server_id}"))?;
        Ok(conn.tools.iter().map(|t| McpToolInfo {
            name: t.name.to_string(),
            description: t.description.as_deref().map(|s| s.to_string()),
        }).collect())
    }

    async fn test_mcp_server(&self, server_id: &str) -> anyhow::Result<usize> {
        // Disconnect then reconnect to test
        let mut registry = self.mcp_registry.lock().await;
        let _ = registry.disconnect(server_id).await;
        registry.connect(server_id).await?;
        let tool_count = registry.get_connection(server_id)
            .map(|c| c.tools.len())
            .unwrap_or(0);
        Ok(tool_count)
    }

    async fn update_mcp_server_settings(
        &self,
        server_id: &str,
        request: UpdateMcpServerRequest,
    ) -> anyhow::Result<()> {
        let mut registry = self.mcp_registry.lock().await;
        if let Some(config) = registry.config_mut().servers.get_mut(server_id) {
            if let Some(name) = request.name { config.name = name; }
            if let Some(url) = request.url { config.url = url; }
            if let Some(auto_connect) = request.auto_connect { config.auto_connect = auto_connect; }
            if let Some(auto_retry) = request.auto_retry { config.auto_retry = auto_retry; }
        }
        registry.save_config()?;
        Ok(())
    }

    async fn stop_mcp_retry(&self, server_id: &str) -> anyhow::Result<()> {
        let mut registry = self.mcp_registry.lock().await;
        registry.cancel_retry(server_id);
        Ok(())
    }

    async fn start_mcp_retry(&self, server_id: &str) -> anyhow::Result<()> {
        let registry = self.mcp_registry.lock().await;
        let config = registry.config().servers.get(server_id)
            .ok_or_else(|| anyhow::anyhow!("server not found: {server_id}"))?
            .clone();
        drop(registry);

        simply_core::mcp::spawn_retry_task(
            Arc::clone(&self.mcp_registry),
            server_id.to_string(),
            config,
            None,
        );
        Ok(())
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

    async fn list_providers(&self) -> Vec<llm::ProviderInfo> {
        llm::list_providers().to_vec()
    }

    async fn default_model_id(&self) -> String {
        self.default_model_id.lock().await.clone()
    }

    async fn set_default_model(&self, model_id: &str) -> anyhow::Result<()> {
        // Validate the model ID is valid
        let _ = llm::create_model(model_id)?;
        *self.default_model_id.lock().await = model_id.to_string();
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
