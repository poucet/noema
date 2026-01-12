//! DB-agnostic Session implementation
//!
//! Session<S: TurnStore> provides:
//! - Runtime state management (conversation_id, view_id, cache)
//! - Pending message buffer for uncommitted changes
//! - Lazy resolution of assets/documents for LLM
//! - Display access via cached ResolvedContent

use anyhow::Result;
use llm::{ChatMessage, ChatPayload, ContentBlock};

use crate::storage::content::StoredContent;
use crate::storage::conversation::{MessageRole, SpanRole, TurnStore, TurnWithContent};
use crate::storage::ids::{ConversationId, SpanId, TurnId, ViewId};

use super::resolver::{AssetResolver, ContentBlockResolver};
use super::types::{PendingMessage, ResolvedContent, ResolvedMessage};

// ============================================================================
// Session
// ============================================================================

/// Runtime session state - DB-agnostic
///
/// Generic over TurnStore implementation. Session is just runtime state:
/// conversation context, current view, cached resolved messages.
pub struct Session<S: TurnStore> {
    store: S,
    conversation_id: ConversationId,
    view_id: ViewId,
    /// Cached resolved messages (text resolved, assets/docs cached lazily)
    cache: Vec<ResolvedMessage>,
    /// Pending messages not yet committed
    pending: Vec<PendingMessage>,
}

impl<S: TurnStore> Session<S> {
    /// Open a session for an existing conversation
    ///
    /// Loads messages from the main view and resolves text content once.
    pub async fn open<R: ContentBlockResolver>(
        store: S,
        conversation_id: ConversationId,
        resolver: &R,
    ) -> Result<Self> {
        let view_id = match store.get_main_view(&conversation_id).await? {
            Some(v) => v.id,
            None => store.create_view(&conversation_id, Some("main"), true).await?.id,
        };

        // Load and resolve text once
        let path = store.get_view_path(&view_id).await?;
        let cache = resolve_path(&path, resolver).await?;

        Ok(Self {
            store,
            conversation_id,
            view_id,
            cache,
            pending: Vec::new(),
        })
    }

    /// Create a new session for a new conversation (not yet persisted)
    pub fn new(store: S, conversation_id: ConversationId, view_id: ViewId) -> Self {
        Self {
            store,
            conversation_id,
            view_id,
            cache: Vec::new(),
            pending: Vec::new(),
        }
    }

    /// Get the conversation ID
    pub fn conversation_id(&self) -> &ConversationId {
        &self.conversation_id
    }

    /// Get the current view ID
    pub fn view_id(&self) -> &ViewId {
        &self.view_id
    }

    /// Get reference to the underlying store
    pub fn store(&self) -> &S {
        &self.store
    }

    /// Add a message to pending (not committed yet)
    pub fn add(&mut self, role: MessageRole, content: Vec<StoredContent>) {
        self.pending.push(PendingMessage::new(role, content));
    }

    /// Add a pending message
    pub fn add_pending(&mut self, message: PendingMessage) {
        self.pending.push(message);
    }

    /// Get pending messages
    pub fn pending(&self) -> &[PendingMessage] {
        &self.pending
    }

    /// Clear pending messages without committing
    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }

    /// Commit pending messages as a turn
    ///
    /// Creates a turn with a span containing all pending messages,
    /// resolves text content, and adds to cache.
    pub async fn commit<R: ContentBlockResolver>(
        &mut self,
        role: SpanRole,
        model_id: Option<&str>,
        resolver: &R,
    ) -> Result<TurnId> {
        if self.pending.is_empty() {
            return Err(anyhow::anyhow!("No pending messages to commit"));
        }

        let turn = self.store.add_turn(&self.conversation_id, role).await?;
        let span = self.store.add_span(&turn.id, model_id).await?;

        for msg in self.pending.drain(..) {
            self.store.add_message(&span.id, msg.role, &msg.content).await?;
            // Resolve text and cache
            let resolved = resolve_stored_content(&msg.content, resolver).await?;
            self.cache.push(ResolvedMessage::new(msg.role, resolved));
        }

        self.store.select_span(&self.view_id, &turn.id, &span.id).await?;
        Ok(turn.id)
    }

    /// Commit parallel responses (multiple model responses at one turn)
    ///
    /// Creates a single turn with multiple spans, one per model.
    /// Returns (turn_id, vec of span_ids).
    pub async fn commit_parallel<R: ContentBlockResolver>(
        &mut self,
        responses: Vec<(String, Vec<PendingMessage>)>,
        selected_index: usize,
        resolver: &R,
    ) -> Result<(TurnId, Vec<SpanId>)> {
        if responses.is_empty() {
            return Err(anyhow::anyhow!("No responses to commit"));
        }

        let turn = self.store.add_turn(&self.conversation_id, SpanRole::Assistant).await?;
        let mut span_ids = Vec::with_capacity(responses.len());

        for (idx, (model_id, messages)) in responses.into_iter().enumerate() {
            let span = self.store.add_span(&turn.id, Some(&model_id)).await?;
            span_ids.push(span.id.clone());

            for msg in &messages {
                self.store.add_message(&span.id, msg.role, &msg.content).await?;
            }

            // Select this span if it's the selected one, and cache its messages
            if idx == selected_index {
                self.store.select_span(&self.view_id, &turn.id, &span.id).await?;
                for msg in messages {
                    let resolved = resolve_stored_content(&msg.content, resolver).await?;
                    self.cache.push(ResolvedMessage::new(msg.role, resolved));
                }
            }
        }

        Ok((turn.id, span_ids))
    }

    /// Get messages for display - returns cached ResolvedContent directly
    ///
    /// This is a sync operation since content is already resolved.
    pub fn messages_for_display(&self) -> &[ResolvedMessage] {
        &self.cache
    }

    /// Get messages for LLM - resolves assets/docs on first access, caches in-place
    ///
    /// This method requires &mut self because it caches resolution results
    /// in the ResolvedContent variants.
    pub async fn messages_for_llm<R: AssetResolver>(
        &mut self,
        resolver: &R,
    ) -> Result<Vec<ChatMessage>> {
        let mut messages = Vec::with_capacity(self.cache.len());

        for msg in &mut self.cache {
            let blocks = resolve_message_for_llm(msg, resolver).await?;
            let role = msg.role.into();
            messages.push(ChatMessage::new(role, ChatPayload::new(blocks)));
        }

        Ok(messages)
    }

    /// Clear the session cache (used when switching views)
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Get message count (cached + pending)
    pub fn len(&self) -> usize {
        self.cache.len() + self.pending.len()
    }

    /// Check if session is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty() && self.pending.is_empty()
    }
}

// ============================================================================
// Resolution helpers
// ============================================================================

/// Resolve a view path to ResolvedMessages
async fn resolve_path<R: ContentBlockResolver>(
    path: &[TurnWithContent],
    resolver: &R,
) -> Result<Vec<ResolvedMessage>> {
    let mut messages = Vec::new();

    for turn in path {
        for msg in &turn.messages {
            let content_refs: Vec<StoredContent> = msg
                .content
                .iter()
                .map(|c| c.content.clone())
                .collect();

            let resolved = resolve_stored_content(&content_refs, resolver).await?;
            messages.push(ResolvedMessage::new(msg.message.role, resolved));
        }
    }

    Ok(messages)
}

/// Resolve StoredContent to ResolvedContent (text lookup only)
async fn resolve_stored_content<R: ContentBlockResolver>(
    content: &[StoredContent],
    resolver: &R,
) -> Result<Vec<ResolvedContent>> {
    let mut resolved = Vec::with_capacity(content.len());

    for item in content {
        let r = match item {
            StoredContent::TextRef { content_block_id } => {
                let text = resolver.get_text(content_block_id).await?;
                ResolvedContent::text(text)
            }
            StoredContent::AssetRef {
                asset_id,
                mime_type,
                filename,
            } => ResolvedContent::asset(asset_id, mime_type, filename.clone()),
            StoredContent::DocumentRef { document_id } => {
                ResolvedContent::document(document_id)
            }
            StoredContent::ToolCall(call) => ResolvedContent::tool_call(call.clone()),
            StoredContent::ToolResult(result) => ResolvedContent::tool_result(result.clone()),
        };
        resolved.push(r);
    }

    Ok(resolved)
}

/// Resolve a ResolvedMessage to ContentBlocks for LLM
async fn resolve_message_for_llm<R: AssetResolver>(
    msg: &mut ResolvedMessage,
    resolver: &R,
) -> Result<Vec<ContentBlock>> {
    let mut blocks = Vec::with_capacity(msg.content.len());

    for item in &mut msg.content {
        let block = match item {
            ResolvedContent::Text { text } => ContentBlock::Text { text: text.clone() },
            ResolvedContent::Asset {
                asset_id,
                mime_type,
                resolved,
                ..
            } => {
                match resolved {
                    Some(cached) => cached.clone(),
                    None => {
                        let block = resolver.resolve_asset(asset_id, mime_type).await?;
                        *resolved = Some(block.clone());
                        block
                    }
                }
            }
            ResolvedContent::Document {
                document_id,
                resolved,
            } => {
                match resolved {
                    Some(cached) => cached.clone(),
                    None => {
                        let block = resolver.resolve_document(document_id).await?;
                        *resolved = Some(block.clone());
                        block
                    }
                }
            }
            ResolvedContent::ToolCall(call) => ContentBlock::ToolCall(call.clone()),
            ResolvedContent::ToolResult(result) => ContentBlock::ToolResult(result.clone()),
        };
        blocks.push(block);
    }

    Ok(blocks)
}

// ============================================================================
// MessageRole -> llm::Role conversion
// ============================================================================

impl From<MessageRole> for llm::Role {
    fn from(role: MessageRole) -> Self {
        match role {
            MessageRole::User => llm::Role::User,
            MessageRole::Assistant => llm::Role::Assistant,
            MessageRole::System => llm::Role::System,
            // Tool messages are treated as user messages for LLM API
            MessageRole::Tool => llm::Role::User,
        }
    }
}
