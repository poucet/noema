//! Lightweight in-memory session — no storage dependency.
//!
//! Used for ephemeral sessions (e.g. Lumina Discord chat) where
//! conversation history lives only in memory and is never persisted.

use anyhow::Result;
use async_trait::async_trait;
use llm::{ChatMessage, ChatPayload, ContentBlock, Role};

use crate::context::{ConversationContext, MessagesGuard};
use crate::storage::content::InputContent;

/// A storage-free session that keeps all messages in memory.
///
/// Suitable for ephemeral conversations where persistence is not needed.
/// Messages are plain `ChatMessage`s — no turns, spans, or storage coordination.
pub struct LightSession {
    messages: Vec<ChatMessage>,
    system_prompt: Option<String>,
}

impl LightSession {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            system_prompt: None,
        }
    }

    pub fn with_system_prompt(system_prompt: String) -> Self {
        Self {
            messages: Vec::new(),
            system_prompt: Some(system_prompt),
        }
    }

    /// Seed with pre-built chat messages (e.g. Discord channel history).
    pub fn seed(&mut self, messages: Vec<ChatMessage>) {
        self.messages.extend(messages);
    }

    /// Add a user message from InputContent (no storage round-trip).
    pub fn add_user_input(&mut self, content: Vec<InputContent>) {
        let blocks: Vec<ContentBlock> = content
            .into_iter()
            .filter_map(|c| c.into_content_block_inline())
            .collect();
        if !blocks.is_empty() {
            self.messages.push(ChatMessage::user(ChatPayload::new(blocks)));
        }
    }
}

#[async_trait]
impl ConversationContext for LightSession {
    async fn messages(&mut self) -> Result<MessagesGuard<'_>> {
        Ok(MessagesGuard::new(&self.messages))
    }

    fn len(&self) -> usize {
        self.messages.len()
    }

    fn add(&mut self, message: ChatMessage) {
        self.messages.push(message);
    }

    fn pending(&self) -> &[ChatMessage] {
        // LightSession has no pending/committed distinction — everything is in messages
        &[]
    }

    fn system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

    async fn commit(&mut self) -> Result<()> {
        // No-op: nothing to persist
        Ok(())
    }
}
