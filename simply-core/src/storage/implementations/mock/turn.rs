//! Mock turn store for testing

use anyhow::Result;
use async_trait::async_trait;
use llm::Role;

use crate::storage::content::StoredContent;
use crate::storage::ids::{ConversationId, MessageId, SpanId, TurnId};
use crate::storage::traits::TurnStore;
use crate::storage::types::{Message, MessageWithContent, Span, Stored, Turn, TurnWithContent};

/// Mock turn store that returns unimplemented for all operations
pub struct MockTurnStore;

#[async_trait]
impl TurnStore for MockTurnStore {
    async fn create_turn(&self, _: llm::Role) -> Result<Stored<TurnId, Turn>> {
        unimplemented!()
    }
    async fn get_turn(&self, _: &TurnId) -> Result<Option<Stored<TurnId, Turn>>> {
        unimplemented!()
    }
    async fn create_span(&self, _: &TurnId, _: Option<&str>) -> Result<Stored<SpanId, Span>> {
        unimplemented!()
    }
    async fn get_spans(&self, _: &TurnId) -> Result<Vec<Stored<SpanId, Span>>> {
        unimplemented!()
    }
    async fn get_span(&self, _: &SpanId) -> Result<Option<Stored<SpanId, Span>>> {
        unimplemented!()
    }
    async fn add_message(
        &self,
        _: &SpanId,
        _: Role,
        _: &[StoredContent],
    ) -> Result<Stored<MessageId, Message>> {
        unimplemented!()
    }
    async fn get_messages(&self, _: &SpanId) -> Result<Vec<MessageWithContent>> {
        unimplemented!()
    }
    async fn get_message(&self, _: &MessageId) -> Result<Option<Stored<MessageId, Message>>> {
        unimplemented!()
    }
    async fn select_span(&self, _: &ConversationId, _: &TurnId, _: &SpanId) -> Result<()> {
        unimplemented!()
    }
    async fn get_selected_span(&self, _: &ConversationId, _: &TurnId) -> Result<Option<SpanId>> {
        unimplemented!()
    }
    async fn get_conversation_path(&self, _: &ConversationId) -> Result<Vec<TurnWithContent>> {
        unimplemented!()
    }
    async fn get_context_at(&self, _: &ConversationId, _: &TurnId) -> Result<Vec<TurnWithContent>> {
        unimplemented!()
    }
    async fn copy_selections(
        &self,
        _: &ConversationId,
        _: &ConversationId,
        _: &TurnId,
        _: bool,
    ) -> Result<usize> {
        unimplemented!()
    }
    async fn get_turn_count(&self, _: &ConversationId) -> Result<usize> {
        unimplemented!()
    }
}
