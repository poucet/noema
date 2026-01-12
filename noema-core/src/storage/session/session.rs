//! DB-agnostic Session implementation
//!
//! Session<T, C> provides:
//! - Runtime state management (conversation_id, view_id, cache)
//! - Pending message buffer for uncommitted changes
//! - Lazy resolution of assets/documents for LLM
//! - Implements ConversationContext directly

use anyhow::Result;
use async_trait::async_trait;
use llm::{ChatMessage, ChatPayload, ContentBlock};
use std::sync::Arc;

use crate::context::{ConversationContext, MessagesGuard};
use crate::storage::content::StoredContent;
use crate::storage::ids::{ConversationId, ViewId};
use crate::storage::traits::{ContentBlockStore, TurnStore};
use crate::storage::types::{ContentBlock as ContentBlockData, ContentOrigin, MessageRole, OriginKind, SpanRole, TurnWithContent};

use super::types::{ResolvedContent, ResolvedMessage};

// ============================================================================
// Session
// ============================================================================

/// Runtime session state - DB-agnostic
///
/// Generic over TurnStore (T) and ContentBlockStore (C) implementations.
/// These can be the same type (e.g., SqliteStore) or different backends.
/// Session is runtime state: conversation context, current view, cached resolved messages.
/// Implements ConversationContext for direct use with agents.
pub struct Session<T: TurnStore, C: ContentBlockStore> {
    turn_store: Arc<T>,
    content_store: Arc<C>,
    conversation_id: ConversationId,
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

impl<T: TurnStore + Send + Sync, C: ContentBlockStore + Send + Sync> Session<T, C> {
    /// Open a session for an existing conversation
    pub async fn open(
        turn_store: Arc<T>,
        content_store: Arc<C>,
        conversation_id: ConversationId,
    ) -> Result<Self> {
        let view_id = match turn_store.get_main_view(&conversation_id).await? {
            Some(v) => v.id,
            None => turn_store.create_view(&conversation_id, Some("main"), true).await?.id,
        };

        let path = turn_store.get_view_path(&view_id).await?;
        let resolved_cache = resolve_path(&path, content_store.as_ref()).await?;

        Ok(Self {
            turn_store,
            content_store,
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
        turn_store: Arc<T>,
        content_store: Arc<C>,
        conversation_id: ConversationId,
        view_id: ViewId,
    ) -> Self {
        Self {
            turn_store,
            content_store,
            conversation_id,
            view_id,
            resolved_cache: Vec::new(),
            llm_cache: Vec::new(),
            llm_cache_valid: false,
            pending: Vec::new(),
        }
    }

    pub fn conversation_id(&self) -> &ConversationId {
        &self.conversation_id
    }

    pub fn view_id(&self) -> &ViewId {
        &self.view_id
    }

    pub fn turn_store(&self) -> &Arc<T> {
        &self.turn_store
    }

    pub fn content_store(&self) -> &Arc<C> {
        &self.content_store
    }

    /// Get committed messages for display - returns cached ResolvedContent
    pub fn messages_for_display(&self) -> &[ResolvedMessage] {
        &self.resolved_cache
    }

    /// Get pending (uncommitted) messages
    pub fn pending_messages(&self) -> &[ChatMessage] {
        &self.pending
    }

    /// Clear the session cache (used when switching views)
    pub fn clear_cache(&mut self) {
        self.resolved_cache.clear();
        self.llm_cache.clear();
        self.llm_cache_valid = false;
    }

    /// Clear pending messages without committing
    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }

    /// Commit pending messages to storage
    ///
    /// Converts ChatMessages to StoredContent, stores text in content_store,
    /// creates turns/spans/messages in turn_store, and updates cache.
    pub async fn commit(&mut self, model_id: Option<&str>) -> Result<()> {
        if self.pending.is_empty() {
            return Ok(()); // Nothing to commit
        }

        // Take pending messages to avoid borrow conflict
        let messages: Vec<ChatMessage> = std::mem::take(&mut self.pending);

        // Group messages by role to create appropriate turns
        // User messages -> User turn, Assistant messages -> Assistant turn
        let mut current_role: Option<SpanRole> = None;
        let mut current_span: Option<crate::storage::ids::SpanId> = None;

        for msg in messages {
            let msg_role = llm_role_to_message_role(msg.role);
            let span_role = match msg_role {
                MessageRole::User | MessageRole::System => SpanRole::User,
                MessageRole::Assistant | MessageRole::Tool => SpanRole::Assistant,
            };

            // Create new turn if role changed
            if current_role != Some(span_role) {
                let turn = self.turn_store.add_turn(&self.conversation_id, span_role).await?;
                let span = self.turn_store.add_span(&turn.id, model_id).await?;
                self.turn_store.select_span(&self.view_id, &turn.id, &span.id).await?;
                current_span = Some(span.id);
                current_role = Some(span_role);
            }

            let span_id = current_span.as_ref().unwrap();

            // Convert ChatMessage content to StoredContent
            let stored = self.store_chat_content(&msg).await?;

            // Add message to turn store
            self.turn_store.add_message(span_id, msg_role, &stored).await?;

            // Resolve and cache for display
            let resolved = resolve_stored_content(&stored, self.content_store.as_ref()).await?;
            self.resolved_cache.push(ResolvedMessage::new(msg_role, resolved));
        }

        self.llm_cache_valid = false;
        Ok(())
    }

    /// Convert ChatMessage content blocks to StoredContent
    async fn store_chat_content(&self, message: &ChatMessage) -> Result<Vec<StoredContent>> {
        let mut stored = Vec::with_capacity(message.payload.content.len());
        let origin = match message.role {
            llm::Role::User => OriginKind::User,
            llm::Role::Assistant => OriginKind::Assistant,
            llm::Role::System => OriginKind::System,
        };

        for block in &message.payload.content {
            let s = match block {
                ContentBlock::Text { text } => {
                    // Store text in content_store
                    let content_origin = ContentOrigin { kind: Some(origin), ..Default::default() };
                    let content_block = ContentBlockData::plain(text).with_origin(content_origin);
                    let result = self.content_store.store(content_block).await?;
                    StoredContent::text_ref(result.id)
                }
                ContentBlock::Image { data, mime_type } => {
                    // For now, store inline (TODO: use blob store)
                    StoredContent::AssetRef {
                        asset_id: format!("inline:{}", &data[..20.min(data.len())]),
                        mime_type: mime_type.clone(),
                        filename: None,
                    }
                }
                ContentBlock::Audio { data, mime_type } => {
                    StoredContent::AssetRef {
                        asset_id: format!("inline:{}", &data[..20.min(data.len())]),
                        mime_type: mime_type.clone(),
                        filename: None,
                    }
                }
                ContentBlock::DocumentRef { id, .. } => StoredContent::document_ref(id),
                ContentBlock::ToolCall(call) => StoredContent::ToolCall(call.clone()),
                ContentBlock::ToolResult(result) => StoredContent::ToolResult(result.clone()),
            };
            stored.push(s);
        }

        Ok(stored)
    }
}

// ============================================================================
// ConversationContext Implementation
// ============================================================================

#[async_trait]
impl<T: TurnStore + Send + Sync, C: ContentBlockStore + Send + Sync> ConversationContext for Session<T, C> {
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
        // Delegate to the concrete commit method
        Session::commit(self, None).await
    }
}

// ============================================================================
// Resolution helpers
// ============================================================================

async fn resolve_path<R: ContentBlockStore>(
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

async fn resolve_stored_content<R: ContentBlockStore>(
    content: &[StoredContent],
    resolver: &R,
) -> Result<Vec<ResolvedContent>> {
    let mut resolved = Vec::with_capacity(content.len());

    for item in content {
        let r = match item {
            StoredContent::TextRef { content_block_id } => {
                let text = resolver.require_text(content_block_id).await?;
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

fn llm_role_to_message_role(role: llm::Role) -> MessageRole {
    match role {
        llm::Role::User => MessageRole::User,
        llm::Role::Assistant => MessageRole::Assistant,
        llm::Role::System => MessageRole::System,
    }
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
            MessageRole::Tool => llm::Role::User,
        }
    }
}
