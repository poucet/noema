//! In-process implementation of [`DaemonApi`].
//!
//! `EmbeddedDaemon` hosts the full daemon logic inside the calling process —
//! no networking, no separate binary. This is the first (and simplest)
//! implementation and the one Noema uses until the WebSocket layer is built.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, Mutex};

use llm::ChatModel;

use simply_core::storage::coordinator::StorageCoordinator;
use simply_core::storage::ids::{ConversationId, UserId};
use simply_core::storage::session::Session;
use simply_core::storage::traits::{StorageTypes, Stores};
use simply_core::storage::DocumentResolver;
use simply_core::{ConversationManager, ManagerEvent, McpRegistry, SharedEventSender};

use crate::api::*;

// ---------------------------------------------------------------------------
// Session bookkeeping
// ---------------------------------------------------------------------------

struct ManagedSession<S: StorageTypes> {
    info: SessionInfo,
    manager: ConversationManager<S>,
}

// ---------------------------------------------------------------------------
// EmbeddedDaemon
// ---------------------------------------------------------------------------

/// In-process daemon — all operations are direct Rust calls, no networking.
///
/// Generic over `StorageTypes` so callers choose the storage backend
/// (e.g. `AppStorage` with SQLite in Noema, `MemoryStorage` in tests).
pub struct EmbeddedDaemon<S: StorageTypes> {
    coordinator: Arc<StorageCoordinator<S>>,
    stores: Arc<dyn Stores<S>>,
    mcp_registry: Arc<Mutex<McpRegistry>>,
    sessions: Mutex<HashMap<SessionId, ManagedSession<S>>>,
    model: Mutex<Arc<dyn ChatModel + Send + Sync>>,
    model_id: Mutex<String>,
    user_id: UserId,
    event_tx: SharedEventSender,
    /// Receiver end — caller takes this once to consume events.
    event_rx: Mutex<Option<mpsc::UnboundedReceiver<(ConversationId, ManagerEvent)>>>,
}

impl<S: StorageTypes> EmbeddedDaemon<S>
where
    S::Document: DocumentResolver,
{
    /// Create a new in-process daemon.
    ///
    /// `stores` must implement `Stores<S>` where `S::Document` implements
    /// `DocumentResolver` (SQLite satisfies this automatically).
    pub fn new(
        coordinator: Arc<StorageCoordinator<S>>,
        stores: Arc<dyn Stores<S>>,
        mcp_registry: Arc<Mutex<McpRegistry>>,
        model: Arc<dyn ChatModel + Send + Sync>,
        model_id: String,
        user_id: UserId,
    ) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Self {
            coordinator,
            stores,
            mcp_registry,
            sessions: Mutex::new(HashMap::new()),
            model: Mutex::new(model),
            model_id: Mutex::new(model_id),
            user_id,
            event_tx,
            event_rx: Mutex::new(Some(event_rx)),
        }
    }

    /// Take the event receiver. Can only be called once — the caller owns the
    /// stream and is responsible for dispatching events to the UI or wherever.
    pub async fn take_event_receiver(
        &self,
    ) -> Option<mpsc::UnboundedReceiver<(ConversationId, ManagerEvent)>> {
        self.event_rx.lock().await.take()
    }

    /// Get a clone of the event sender (for passing to subsystems).
    pub fn event_sender(&self) -> SharedEventSender {
        self.event_tx.clone()
    }
}

// ---------------------------------------------------------------------------
// DaemonApi implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl<S: StorageTypes> DaemonApi for EmbeddedDaemon<S>
where
    S::Document: DocumentResolver,
{
    // -- Session lifecycle ---------------------------------------------------

    async fn create_session(&self, options: CreateSessionOptions) -> anyhow::Result<SessionId> {
        let conversation_id = ConversationId::new();
        let session_id = conversation_id.as_str().to_string();

        let persistence = options.persistence.unwrap_or(Persistence::Persistent);

        // Resolve model — use per-session override or daemon default
        let model = self.model.lock().await.clone();
        let model_id = match options.model_id {
            Some(id) => id,
            None => self.model_id.lock().await.clone(),
        };

        // Create the session and conversation manager
        let session = Session::new(
            Arc::clone(&self.coordinator),
            conversation_id.clone(),
        );

        let document_resolver: Arc<dyn DocumentResolver> = self.stores.document();

        let manager = ConversationManager::new(
            session,
            Arc::clone(&self.coordinator),
            model,
            model_id.clone(),
            Arc::clone(&self.mcp_registry),
            document_resolver,
            self.user_id.clone(),
            self.event_tx.clone(),
        );

        let info = SessionInfo {
            id: session_id.clone(),
            persistence,
            model_id,
            created_at: {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                format!("{}", now.as_secs())
            },
        };

        self.sessions.lock().await.insert(
            session_id.clone(),
            ManagedSession { info, manager },
        );

        tracing::info!(session_id = %session_id, "session created");
        Ok(session_id)
    }

    async fn resume_session(&self, session_id: &str) -> anyhow::Result<SessionInfo> {
        let sessions = self.sessions.lock().await;
        let managed = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;
        Ok(managed.info.clone())
    }

    async fn close_session(&self, session_id: &str) -> anyhow::Result<()> {
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
        _session_id: &str,
        _messages: Vec<SeedMessage>,
    ) -> anyhow::Result<()> {
        // TODO: convert SeedMessages to ChatMessages and add to session pending
        tracing::warn!("seed_context not yet implemented");
        Ok(())
    }

    async fn list_sessions(&self) -> anyhow::Result<Vec<SessionInfo>> {
        let sessions = self.sessions.lock().await;
        Ok(sessions.values().map(|s| s.info.clone()).collect())
    }

    async fn set_persistence(
        &self,
        session_id: &str,
        persistence: Persistence,
    ) -> anyhow::Result<()> {
        let mut sessions = self.sessions.lock().await;
        let managed = sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;
        managed.info.persistence = persistence;
        Ok(())
    }

    // -- Conversation --------------------------------------------------------

    async fn send_message(
        &self,
        session_id: &str,
        message: UserMessage,
    ) -> anyhow::Result<Vec<DaemonEvent>> {
        let sessions = self.sessions.lock().await;
        let managed = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;

        let tool_config = message
            .tool_filter
            .map(|f| f.into_tool_config())
            .unwrap_or_else(simply_core::ToolConfig::all_enabled);

        // Fire-and-forget — the manager runs in a background task and emits
        // events through the event_tx channel.
        managed.manager.send_message(message.content, tool_config);

        // Return an empty vec for now. The real events flow through
        // take_event_receiver(). A future streaming API will replace this.
        Ok(vec![])
    }

    async fn set_model(&self, session_id: &str, model_id: &str) -> anyhow::Result<()> {
        // For now, just update the session info. Full model resolution
        // (provider lookup, creating a ChatModel instance) will be added
        // when we wire the model registry.
        let mut sessions = self.sessions.lock().await;
        let managed = sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;
        managed.info.model_id = model_id.to_string();
        Ok(())
    }

    async fn truncate(&self, session_id: &str, _before_turn: Option<&str>) -> anyhow::Result<()> {
        let sessions = self.sessions.lock().await;
        let managed = sessions
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_id}"))?;
        managed.manager.clear_history();
        Ok(())
    }

    // -- Assets --------------------------------------------------------------

    async fn upload_asset(
        &self,
        data: Vec<u8>,
        media_type: &str,
    ) -> anyhow::Result<simply_core::storage::ids::AssetId> {
        use base64::{engine::general_purpose::STANDARD, Engine};
        let b64 = STANDARD.encode(&data);
        self.coordinator.store_asset(&b64, media_type).await
    }

    // -- MCP tools -----------------------------------------------------------

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

    // -- Events --------------------------------------------------------------

    async fn push_event(&self, event: InboundEvent) -> anyhow::Result<()> {
        tracing::info!(event_type = %event.event_type, "inbound event received (not yet routed)");
        // TODO: route events to appropriate session or handler
        Ok(())
    }

    // -- Voice ---------------------------------------------------------------

    async fn voice_connect(&self, _session_id: &str) -> anyhow::Result<VoiceHandle> {
        // Voice pipeline integration is a later task — stub with unconnected channels
        let (audio_tx, _audio_rx) = mpsc::channel(32);
        let (_voice_tx, voice_rx) = mpsc::channel(32);
        Ok(VoiceHandle {
            audio_in: audio_tx,
            events: voice_rx,
        })
    }
}
