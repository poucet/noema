//! Mock turn store for testing

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::content::StoredContent;
use crate::storage::ids::{MessageId, SpanId, TurnId, ViewId};
use crate::storage::traits::TurnStore;
use crate::storage::types::{
    MessageInfo, MessageRole, MessageWithContent, SpanInfo, SpanRole, TurnInfo, TurnWithContent,
    ViewInfo,
};

/// Mock turn store that returns unimplemented for all operations
pub struct MockTurnStore;

#[async_trait]
impl TurnStore for MockTurnStore {
    async fn create_turn(&self, _: SpanRole) -> Result<TurnInfo> {
        unimplemented!()
    }
    async fn get_turn(&self, _: &TurnId) -> Result<Option<TurnInfo>> {
        unimplemented!()
    }
    async fn create_span(&self, _: &TurnId, _: Option<&str>) -> Result<SpanInfo> {
        unimplemented!()
    }
    async fn get_spans(&self, _: &TurnId) -> Result<Vec<SpanInfo>> {
        unimplemented!()
    }
    async fn get_span(&self, _: &SpanId) -> Result<Option<SpanInfo>> {
        unimplemented!()
    }
    async fn add_message(
        &self,
        _: &SpanId,
        _: MessageRole,
        _: &[StoredContent],
    ) -> Result<MessageInfo> {
        unimplemented!()
    }
    async fn get_messages(&self, _: &SpanId) -> Result<Vec<MessageWithContent>> {
        unimplemented!()
    }
    async fn get_message(&self, _: &MessageId) -> Result<Option<MessageInfo>> {
        unimplemented!()
    }
    async fn create_view(&self) -> Result<ViewInfo> {
        unimplemented!()
    }
    async fn get_view(&self, _: &ViewId) -> Result<Option<ViewInfo>> {
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
    async fn fork_view(&self, _: &ViewId, _: &TurnId) -> Result<ViewInfo> {
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
    ) -> Result<(SpanInfo, Option<ViewInfo>)> {
        unimplemented!()
    }
}
