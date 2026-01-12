//! DB-agnostic Session implementation
//!
//! Session<S: TurnStore> provides:
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
use crate::storage::ids::{ConversationId, TurnId, ViewId};
use crate::storage::traits::{ContentBlockStore, TurnStore};
use crate::storage::types::{MessageRole, SpanRole, TurnWithContent};

use super::resolver::AssetResolver;
use super::types::{ResolvedContent, ResolvedMessage};

// ============================================================================
// Session
// ============================================================================

/// Runtime session state - DB-agnostic
///
/// Generic over TurnStore implementation. Session is runtime state:
/// conversation context, current view, cached resolved messages.
/// Implements ConversationContext for direct use with agents.
pub struct Session<S: TurnStore> {
    store: Arc<S>,
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

impl<S: TurnStore + Send + Sync> Session<S> {
    /// Open a session for an existing conversation
    pub async fn open<R: ContentBlockStore>(
        store: Arc<S>,
        conversation_id: ConversationId,
        resolver: &R,
    ) -> Result<Self> {
        let view_id = match store.get_main_view(&conversation_id).await? {
            Some(v) => v.id,
            None => store.create_view(&conversation_id, Some("main"), true).await?.id,
        };

        let path = store.get_view_path(&view_id).await?;
        let resolved_cache = resolve_path(&path, resolver).await?;

        Ok(Self {
            store,
            conversation_id,
            view_id,
            resolved_cache,
            llm_cache: Vec::new(),
            llm_cache_valid: false,
            pending: Vec::new(),
        })
    }

    /// Create a new session for a new conversation (not yet persisted)
    pub fn new(store: Arc<S>, conversation_id: ConversationId, view_id: ViewId) -> Self {
        Self {
            store,
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

    pub fn store(&self) -> &Arc<S> {
        &self.store
    }

    /// Get messages for display - returns cached ResolvedContent
    pub fn messages_for_display(&self) -> &[ResolvedMessage] {
        &self.resolved_cache
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

    /// Resolve and cache messages for LLM
    async fn ensure_llm_cache<R: AssetResolver>(&mut self, resolver: &R) -> Result<()> {
        if self.llm_cache_valid {
            return Ok(());
        }

        self.llm_cache.clear();
        self.llm_cache.reserve(self.resolved_cache.len() + self.pending.len());

        for msg in &mut self.resolved_cache {
            let blocks = resolve_message_for_llm(msg, resolver).await?;
            let role = msg.role.into();
            self.llm_cache.push(ChatMessage::new(role, ChatPayload::new(blocks)));
        }

        self.llm_cache_valid = true;
        Ok(())
    }

    /// Commit pending messages to storage
    ///
    /// Converts ChatMessages to StoredContent, stores them, and updates cache.
    pub async fn commit_pending<C, R>(
        &mut self,
        role: SpanRole,
        model_id: Option<&str>,
        coordinator: &C,
        resolver: &R,
    ) -> Result<TurnId>
    where
        C: ContentStorer,
        R: ContentBlockStore,
    {
        if self.pending.is_empty() {
            return Err(anyhow::anyhow!("No pending messages to commit"));
        }

        let turn = self.store.add_turn(&self.conversation_id, role).await?;
        let span = self.store.add_span(&turn.id, model_id).await?;

        for msg in self.pending.drain(..) {
            // Convert ChatMessage to StoredContent
            let stored = coordinator.store_chat_message(&msg).await?;
            let msg_role = llm_role_to_message_role(msg.role);

            self.store.add_message(&span.id, msg_role, &stored).await?;

            // Resolve and cache
            let resolved = resolve_stored_content(&stored, resolver).await?;
            self.resolved_cache.push(ResolvedMessage::new(msg_role, resolved));
        }

        self.store.select_span(&self.view_id, &turn.id, &span.id).await?;
        self.llm_cache_valid = false;

        Ok(turn.id)
    }
}

// ============================================================================
// ConversationContext Implementation
// ============================================================================

#[async_trait]
impl<S: TurnStore + Send + Sync> ConversationContext for Session<S> {
    async fn messages(&mut self) -> Result<MessagesGuard<'_>> {
        // For now, return resolved messages converted to ChatMessage
        // TODO: Use proper AssetResolver here
        if !self.llm_cache_valid {
            self.llm_cache.clear();
            for msg in &self.resolved_cache {
                let blocks = resolved_message_to_blocks(msg);
                let role = msg.role.into();
                self.llm_cache.push(ChatMessage::new(role, ChatPayload::new(blocks)));
            }
            self.llm_cache_valid = true;
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
        // This requires external coordinator/resolver - can't implement here
        // The engine should call commit_pending() with proper dependencies
        Err(anyhow::anyhow!(
            "Use commit_pending() with coordinator and resolver instead"
        ))
    }
}

// ============================================================================
// ContentStorer trait - for converting ChatMessage to StoredContent
// ============================================================================

/// Trait for storing ChatMessage content blocks
#[async_trait]
pub trait ContentStorer: Send + Sync {
    /// Convert a ChatMessage to Vec<StoredContent>
    async fn store_chat_message(&self, message: &ChatMessage) -> Result<Vec<StoredContent>>;
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
