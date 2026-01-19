//! DB-agnostic Session implementation
//!
//! Session provides:
//! - Runtime state management (view_id, cache)
//! - Pending message buffer for uncommitted changes
//! - Lazy resolution of assets/documents for LLM
//! - Implements ConversationContext directly

use anyhow::Result;
use async_trait::async_trait;
use llm::{ChatMessage, ChatPayload, ContentBlock, Role};
use std::sync::Arc;

use crate::context::{ConversationContext, MessagesGuard};
use crate::manager::CommitMode;
use crate::storage::content::InputContent;
use crate::storage::coordinator::StorageCoordinator;
use crate::storage::ids::{ConversationId, SpanId, TurnId, ViewId};
use crate::storage::traits::StorageTypes;
use crate::storage::types::OriginKind;

use super::types::{ResolvedContent, ResolvedMessage};

// ============================================================================
// Session
// ============================================================================

/// Runtime session state - DB-agnostic
///
/// Generic over `S: StorageTypes` which bundles all storage type associations.
///
/// Session is runtime state: conversation context, current view, cached resolved messages.
/// Implements ConversationContext for direct use with agents.
///
/// # Type Derivation
///
/// Define your storage types once via `StorageTypes`:
///
/// ```ignore
/// struct AppStorage;
/// impl StorageTypes for AppStorage {
///     type Blob = FsBlobStore;
///     type Asset = SqliteStore;
///     // ...
/// }
///
/// type AppSession = Session<AppStorage>;
/// type AppEngine = ChatEngine<AppStorage>;
/// ```
pub struct Session<S: StorageTypes> {
    /// Storage coordinator - provides access to all stores
    coordinator: Arc<StorageCoordinator<S>>,
    /// Conversation ID
    conversation_id: ConversationId,
    /// Current view being used
    view_id: ViewId,
    /// Cached resolved messages (text resolved, assets/docs cached lazily)
    resolved_cache: Vec<ResolvedMessage>,
    /// Cached ChatMessages for LLM (lazily populated from resolved_cache)
    llm_cache: Vec<ChatMessage>,
    /// Whether llm_cache is valid
    llm_cache_valid: bool,
    /// Pending messages (ChatMessage) not yet committed
    pending: Vec<ChatMessage>,
}

impl<S: StorageTypes> Session<S> {
    /// Open a session for an existing conversation
    ///
    /// Delegates view resolution to the StorageCoordinator which handles
    /// the multi-store coordination of getting/creating views and resolving content.
    pub async fn open(
        coordinator: Arc<StorageCoordinator<S>>,
        conversation_id: ConversationId,
    ) -> Result<Self> {
        let (view_id, resolved_cache) = coordinator.open_session(&conversation_id).await?;

        Ok(Self {
            coordinator,
            conversation_id,
            view_id,
            resolved_cache,
            llm_cache: Vec::new(),
            llm_cache_valid: false,
            pending: Vec::new(),
        })
    }

    /// Create a new session for a new conversation (not yet persisted)
    pub fn new(
        coordinator: Arc<StorageCoordinator<S>>,
        conversation_id: ConversationId,
        view_id: ViewId,
    ) -> Self {
        Self {
            coordinator,
            conversation_id,
            view_id,
            resolved_cache: Vec::new(),
            llm_cache: Vec::new(),
            llm_cache_valid: false,
            pending: Vec::new(),
        }
    }

    /// Open a session for a specific view
    ///
    /// Use this when switching to a non-main view (e.g., after forking).
    pub async fn open_view(
        coordinator: Arc<StorageCoordinator<S>>,
        conversation_id: ConversationId,
        view_id: ViewId,
    ) -> Result<Self> {
        let resolved_cache = coordinator.open_session_with_view(&view_id).await?;

        Ok(Self {
            coordinator,
            conversation_id,
            view_id,
            resolved_cache,
            llm_cache: Vec::new(),
            llm_cache_valid: false,
            pending: Vec::new(),
        })
    }

    pub fn conversation_id(&self) -> &ConversationId {
        &self.conversation_id
    }

    pub fn view_id(&self) -> &ViewId {
        &self.view_id
    }
    
    /// Get committed messages for display - returns cached ResolvedContent
    pub fn messages_for_display(&self) -> &[ResolvedMessage] {
        &self.resolved_cache
    }

    /// Get pending (uncommitted) messages
    pub fn pending_messages(&self) -> &[ChatMessage] {
        &self.pending
    }

    /// Get all messages (committed + pending) as ChatMessages for display
    /// This combines resolved messages converted to ChatMessages with pending messages.
    pub fn all_messages(&self) -> Vec<ChatMessage> {
        let mut result = Vec::with_capacity(self.resolved_cache.len() + self.pending.len());

        // Convert resolved messages to ChatMessages
        for msg in &self.resolved_cache {
            let blocks = resolved_message_to_blocks(msg);
            let role = msg.role.into();
            result.push(ChatMessage::new(role, ChatPayload::new(blocks)));
        }

        // Add pending messages
        for msg in &self.pending {
            result.push(msg.clone());
        }

        result
    }

    /// Clear the session cache (used when switching views)
    pub fn clear_cache(&mut self) {
        self.resolved_cache.clear();
        self.llm_cache.clear();
        self.llm_cache_valid = false;
    }

    /// Reload messages from storage for current view
    pub async fn reload(&mut self) -> Result<()> {
        let resolved_cache = self.coordinator.open_session_with_view(&self.view_id).await?;
        self.resolved_cache = resolved_cache;
        self.llm_cache.clear();
        self.llm_cache_valid = false;
        Ok(())
    }

    /// Clear pending messages without committing
    pub fn clear_pending(&mut self) {
        self.pending.clear();
        self.llm_cache_valid = false;
    }

    /// Add a resolved message to the cache (used by ConversationManager after commit)
    pub fn add_resolved(&mut self, msg: ResolvedMessage) {
        self.resolved_cache.push(msg);
        self.llm_cache_valid = false;
    }

    /// Truncate session context.
    ///
    /// If `turn_id` is Some, truncates to before that turn (keeping messages from earlier turns).
    /// If `turn_id` is None, clears all context.
    ///
    /// Works entirely in-memory on the resolved cache.
    pub fn truncate(&mut self, turn_id: Option<&TurnId>) {
        match turn_id {
            Some(tid) => {
                // Find first message with this turn_id and truncate there
                if let Some(pos) = self.resolved_cache.iter().position(|msg| &msg.turn_id == tid) {
                    self.resolved_cache.truncate(pos);
                }
            }
            None => {
                self.resolved_cache.clear();
            }
        }
        self.llm_cache.clear();
        self.llm_cache_valid = false;
        self.pending.clear();
    }

    /// Add a user message from UI input
    ///
    /// Stores content (text, images, audio) and adds to pending queue.
    /// The message will be sent to the LLM and committed on success.
    pub async fn add_user_message(&mut self, content: Vec<InputContent>) -> Result<()> {
        if content.is_empty() {
            return Ok(());
        }

        // Store content and get refs
        let stored = self.coordinator
            .store_input_content(content, OriginKind::User)
            .await?;

        // Resolve refs back to ContentBlocks for the pending ChatMessage
        // (We just stored them, so resolution will succeed)
        let mut blocks = Vec::with_capacity(stored.len());
        for item in stored {
            let block = item.resolve(self.coordinator.as_ref()).await?;
            blocks.push(block);
        }

        if !blocks.is_empty() {
            let message = ChatMessage::user(ChatPayload::new(blocks));
            self.pending.push(message);
            self.llm_cache_valid = false;
        }

        Ok(())
    }

    /// Commit pending messages to storage.
    ///
    /// # Arguments
    /// * `model_id` - The model that generated assistant messages
    /// * `commit_mode` - How to commit: NewTurns (create turns) or AtTurn (add span at existing turn)
    pub async fn commit(&mut self, model_id: Option<&str>, commit_mode: &CommitMode) -> Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }

        let messages = std::mem::take(&mut self.pending);

        // Track current turn and span for adding messages
        let mut current_turn: Option<TurnId> = None;
        let mut current_span: Option<SpanId> = None;
        let mut current_role: Option<Role> = None;

        for msg in messages {
            let origin = OriginKind::from(msg.role);
            let span_role = msg.role;

            // Get or create turn and span based on commit mode and role changes
            match commit_mode {
                CommitMode::AtTurn(tid) => {
                    // Regeneration: create span at existing turn once, reuse for all messages
                    if current_span.is_none() {
                        let span = self.coordinator
                            .create_and_select_span(&self.view_id, tid, model_id)
                            .await?;
                        current_turn = Some(tid.clone());
                        current_span = Some(span);
                    }
                }
                CommitMode::NewTurns => {
                    // Normal: create new turn when role changes
                    if current_role != Some(span_role) {
                        let tid = self.coordinator.create_turn(span_role).await?;
                        let span = self.coordinator
                            .create_and_select_span(&self.view_id, &tid, model_id)
                            .await?;
                        current_turn = Some(tid);
                        current_span = Some(span);
                        current_role = Some(span_role);
                    }
                }
            };
            let turn_id = current_turn.as_ref().unwrap();
            let span_id = current_span.as_ref().unwrap();

            let resolved = self.coordinator
                .add_message(span_id, turn_id, msg.role, msg.payload.content, origin)
                .await?;
            self.resolved_cache.push(resolved);
        }

        self.llm_cache_valid = false;
        Ok(())
    }
}

// ============================================================================
// ConversationContext Implementation
// ============================================================================

#[async_trait]
impl<S: StorageTypes> ConversationContext for Session<S> {
    async fn messages(&mut self) -> Result<MessagesGuard<'_>> {
        // Rebuild llm_cache if invalid or pending messages changed
        let needs_rebuild = !self.llm_cache_valid ||
            self.llm_cache.len() != self.resolved_cache.len() + self.pending.len();

        if needs_rebuild {
            self.llm_cache.clear();
            self.llm_cache.reserve(self.resolved_cache.len() + self.pending.len());

            // Add resolved (committed) messages
            for msg in &self.resolved_cache {
                let blocks = resolved_message_to_blocks(msg);
                let role = msg.role.into();
                self.llm_cache.push(ChatMessage::new(role, ChatPayload::new(blocks)));
            }

            // Add pending (uncommitted) messages
            for msg in &self.pending {
                self.llm_cache.push(msg.clone());
            }

            self.llm_cache_valid = self.pending.is_empty();
        }

        Ok(MessagesGuard::new(&self.llm_cache))
    }

    fn len(&self) -> usize {
        self.resolved_cache.len() + self.pending.len()
    }

    fn add(&mut self, message: ChatMessage) {
        self.pending.push(message);
    }

    fn pending(&self) -> &[ChatMessage] {
        &self.pending
    }

    async fn commit(&mut self) -> Result<()> {
        // Delegate to the concrete commit method with default mode
        Session::commit(self, None, &CommitMode::default()).await
    }
}

// ============================================================================
// Conversion helpers
// ============================================================================

/// Convert ResolvedMessage to ContentBlocks (sync, without asset resolution)
fn resolved_message_to_blocks(msg: &ResolvedMessage) -> Vec<ContentBlock> {
    msg.content
        .iter()
        .map(|item| match item {
            ResolvedContent::Text { text } => ContentBlock::Text { text: text.clone() },
            ResolvedContent::Asset {
                asset_id,
                mime_type,
                resolved,
                ..
            } => resolved.clone().unwrap_or_else(|| {
                ContentBlock::Text {
                    text: format!("[Asset: {} ({})]", asset_id, mime_type),
                }
            }),
            ResolvedContent::Document {
                document_id,
                resolved,
            } => resolved.clone().unwrap_or_else(|| {
                ContentBlock::Text {
                    text: format!("[Document: {}]", document_id),
                }
            }),
            ResolvedContent::ToolCall(call) => ContentBlock::ToolCall(call.clone()),
            ResolvedContent::ToolResult(result) => ContentBlock::ToolResult(result.clone()),
        })
        .collect()
}