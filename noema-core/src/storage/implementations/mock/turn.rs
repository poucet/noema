//! Mock turn store for testing

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::content::StoredContent;
use crate::storage::ids::{MessageId, SpanId, TurnId, ViewId};
use crate::storage::traits::TurnStore;
use crate::storage::types::{
    Message, MessageRole, MessageWithContent, Span, Stored, Turn, TurnWithContent, View,
};

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
        _: MessageRole,
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
    async fn create_view(&self) -> Result<Stored<ViewId, View>> {
        unimplemented!()
    }
    async fn get_view(&self, _: &ViewId) -> Result<Option<Stored<ViewId, View>>> {
        unimplemented!()
    }
    async fn list_related_views(&self, _: &ViewId) -> Result<Vec<Stored<ViewId, View>>> {
        unimplemented!()
    }
    async fn select_span(&self, _: &ViewId, _: &TurnId, _: &SpanId) -> Result<()> {
        unimplemented!()
    }
    async fn get_selected_span(&self, _: &ViewId, _: &TurnId) -> Result<Option<SpanId>> {
        unimplemented!()
    }
    async fn get_view_path(&self, _: &ViewId) -> Result<Vec<TurnWithContent>> {
        unimplemented!()
    }
    async fn fork_view(&self, _: &ViewId, _: &TurnId) -> Result<Stored<ViewId, View>> {
        unimplemented!()
    }
    async fn get_view_context_at(&self, _: &ViewId, _: &TurnId) -> Result<Vec<TurnWithContent>> {
        unimplemented!()
    }
    async fn edit_turn(
        &self,
        _: &ViewId,
        _: &TurnId,
        _: Vec<(MessageRole, Vec<StoredContent>)>,
        _: Option<&str>,
        _: bool,
    ) -> Result<(Stored<SpanId, Span>, Option<Stored<ViewId, View>>)> {
        unimplemented!()
    }
}
