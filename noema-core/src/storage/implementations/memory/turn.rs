//! In-memory TurnStore implementation

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::storage::content::StoredContent;
use crate::storage::ids::{
    ConversationId, MessageContentId, MessageId, SpanId, TurnId, ViewId,
};
use crate::storage::traits::TurnStore;
use crate::storage::types::{
    MessageContentInfo, MessageInfo, MessageRole, MessageWithContent, SpanInfo, SpanRole,
    TurnInfo, TurnWithContent, ViewInfo,
};

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// In-memory turn store for testing
#[derive(Debug, Default)]
pub struct MemoryTurnStore {
    turns: Mutex<HashMap<String, TurnInfo>>,
    spans: Mutex<HashMap<String, SpanInfo>>,
    messages: Mutex<HashMap<String, MessageInfo>>,
    message_content: Mutex<HashMap<String, Vec<MessageContentInfo>>>,
    views: Mutex<HashMap<String, ViewInfo>>,
    view_selections: Mutex<HashMap<(String, String), String>>, // (view_id, turn_id) -> span_id
}

impl MemoryTurnStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl TurnStore for MemoryTurnStore {
    // ========== Turn Management ==========

    async fn add_turn(
        &self,
        conversation_id: &ConversationId,
        role: SpanRole,
    ) -> Result<TurnInfo> {
        let mut turns = self.turns.lock().unwrap();

        // Get next sequence number
        let sequence_number = turns
            .values()
            .filter(|t| t.conversation_id == *conversation_id)
            .map(|t| t.sequence_number)
            .max()
            .map(|n| n + 1)
            .unwrap_or(0);

        let turn = TurnInfo {
            id: TurnId::new(),
            conversation_id: conversation_id.clone(),
            role,
            sequence_number,
            created_at: now(),
        };

        turns.insert(turn.id.as_str().to_string(), turn.clone());
        Ok(turn)
    }

    async fn get_turns(&self, conversation_id: &ConversationId) -> Result<Vec<TurnInfo>> {
        let turns = self.turns.lock().unwrap();
        let mut result: Vec<_> = turns
            .values()
            .filter(|t| t.conversation_id == *conversation_id)
            .cloned()
            .collect();
        result.sort_by_key(|t| t.sequence_number);
        Ok(result)
    }

    async fn get_turn(&self, turn_id: &TurnId) -> Result<Option<TurnInfo>> {
        let turns = self.turns.lock().unwrap();
        Ok(turns.get(turn_id.as_str()).cloned())
    }

    // ========== Span Management ==========

    async fn add_span(&self, turn_id: &TurnId, model_id: Option<&str>) -> Result<SpanInfo> {
        let mut spans = self.spans.lock().unwrap();

        let span = SpanInfo {
            id: SpanId::new(),
            turn_id: turn_id.clone(),
            model_id: model_id.map(|s| s.to_string()),
            message_count: 0,
            created_at: now(),
        };

        spans.insert(span.id.as_str().to_string(), span.clone());
        Ok(span)
    }

    async fn get_spans(&self, turn_id: &TurnId) -> Result<Vec<SpanInfo>> {
        let spans = self.spans.lock().unwrap();
        let messages = self.messages.lock().unwrap();

        let mut result: Vec<_> = spans
            .values()
            .filter(|s| s.turn_id == *turn_id)
            .map(|s| {
                let message_count = messages
                    .values()
                    .filter(|m| m.span_id == s.id)
                    .count() as i32;
                SpanInfo {
                    message_count,
                    ..s.clone()
                }
            })
            .collect();
        result.sort_by_key(|s| s.created_at);
        Ok(result)
    }

    async fn get_span(&self, span_id: &SpanId) -> Result<Option<SpanInfo>> {
        let spans = self.spans.lock().unwrap();
        let messages = self.messages.lock().unwrap();

        Ok(spans.get(span_id.as_str()).map(|s| {
            let message_count = messages
                .values()
                .filter(|m| m.span_id == s.id)
                .count() as i32;
            SpanInfo {
                message_count,
                ..s.clone()
            }
        }))
    }

    // ========== Message Management ==========

    async fn add_message(
        &self,
        span_id: &SpanId,
        role: MessageRole,
        content: &[StoredContent],
    ) -> Result<MessageInfo> {
        let mut messages = self.messages.lock().unwrap();
        let mut message_content_map = self.message_content.lock().unwrap();

        // Get next sequence number
        let sequence_number = messages
            .values()
            .filter(|m| m.span_id == *span_id)
            .map(|m| m.sequence_number)
            .max()
            .map(|n| n + 1)
            .unwrap_or(0);

        let message_id = MessageId::new();
        let message = MessageInfo {
            id: message_id.clone(),
            span_id: span_id.clone(),
            sequence_number,
            role,
            created_at: now(),
        };

        // Store content items
        let content_items: Vec<MessageContentInfo> = content
            .iter()
            .enumerate()
            .map(|(seq, c)| MessageContentInfo {
                id: MessageContentId::new(),
                message_id: message_id.clone(),
                sequence_number: seq as i32,
                content: c.clone(),
            })
            .collect();

        messages.insert(message_id.as_str().to_string(), message.clone());
        message_content_map.insert(message_id.as_str().to_string(), content_items);

        Ok(message)
    }

    async fn get_messages(&self, span_id: &SpanId) -> Result<Vec<MessageInfo>> {
        let messages = self.messages.lock().unwrap();
        let mut result: Vec<_> = messages
            .values()
            .filter(|m| m.span_id == *span_id)
            .cloned()
            .collect();
        result.sort_by_key(|m| m.sequence_number);
        Ok(result)
    }

    async fn get_messages_with_content(
        &self,
        span_id: &SpanId,
    ) -> Result<Vec<MessageWithContent>> {
        let messages = self.get_messages(span_id).await?;
        let message_content_map = self.message_content.lock().unwrap();

        let result = messages
            .into_iter()
            .map(|message| {
                let content = message_content_map
                    .get(message.id.as_str())
                    .cloned()
                    .unwrap_or_default();
                MessageWithContent { message, content }
            })
            .collect();

        Ok(result)
    }

    async fn get_message(&self, message_id: &MessageId) -> Result<Option<MessageInfo>> {
        let messages = self.messages.lock().unwrap();
        Ok(messages.get(message_id.as_str()).cloned())
    }

    // ========== View Management ==========

    async fn create_view(
        &self,
        conversation_id: &ConversationId,
        name: Option<&str>,
        is_main: bool,
    ) -> Result<ViewInfo> {
        let mut views = self.views.lock().unwrap();

        let view = ViewInfo {
            id: ViewId::new(),
            conversation_id: conversation_id.clone(),
            name: name.map(|s| s.to_string()),
            is_main,
            forked_from_view_id: None,
            forked_at_turn_id: None,
            created_at: now(),
        };

        views.insert(view.id.as_str().to_string(), view.clone());
        Ok(view)
    }

    async fn get_views(&self, conversation_id: &ConversationId) -> Result<Vec<ViewInfo>> {
        let views = self.views.lock().unwrap();
        let mut result: Vec<_> = views
            .values()
            .filter(|v| v.conversation_id == *conversation_id)
            .cloned()
            .collect();
        result.sort_by_key(|v| v.created_at);
        Ok(result)
    }

    async fn get_main_view(&self, conversation_id: &ConversationId) -> Result<Option<ViewInfo>> {
        let views = self.views.lock().unwrap();
        Ok(views
            .values()
            .find(|v| v.conversation_id == *conversation_id && v.is_main)
            .cloned())
    }

    async fn select_span(
        &self,
        view_id: &ViewId,
        turn_id: &TurnId,
        span_id: &SpanId,
    ) -> Result<()> {
        let mut selections = self.view_selections.lock().unwrap();
        selections.insert(
            (view_id.as_str().to_string(), turn_id.as_str().to_string()),
            span_id.as_str().to_string(),
        );
        Ok(())
    }

    async fn get_selected_span(
        &self,
        view_id: &ViewId,
        turn_id: &TurnId,
    ) -> Result<Option<SpanId>> {
        let selections = self.view_selections.lock().unwrap();
        Ok(selections
            .get(&(view_id.as_str().to_string(), turn_id.as_str().to_string()))
            .map(|s| SpanId::from_string(s.clone())))
    }

    async fn get_view_path(&self, view_id: &ViewId) -> Result<Vec<TurnWithContent>> {
        let conversation_id = {
            let views = self.views.lock().unwrap();
            views
                .get(view_id.as_str())
                .map(|v| v.conversation_id.clone())
                .ok_or_else(|| anyhow::anyhow!("View not found"))?
        };

        let turns = self.get_turns(&conversation_id).await?;
        let mut result = Vec::new();

        for turn in turns {
            let selected_span_id = self.get_selected_span(view_id, &turn.id).await?;

            let span = if let Some(span_id) = selected_span_id {
                self.get_span(&span_id).await?
            } else {
                let spans = self.get_spans(&turn.id).await?;
                spans.into_iter().next()
            };

            if let Some(span) = span {
                let messages = self.get_messages_with_content(&span.id).await?;
                result.push(TurnWithContent {
                    turn,
                    span,
                    messages,
                });
            }
        }

        Ok(result)
    }

    async fn fork_view(
        &self,
        view_id: &ViewId,
        at_turn_id: &TurnId,
        name: Option<&str>,
    ) -> Result<ViewInfo> {
        let (conversation_id, at_turn_seq) = {
            let views = self.views.lock().unwrap();
            let turns = self.turns.lock().unwrap();
            let view = views
                .get(view_id.as_str())
                .ok_or_else(|| anyhow::anyhow!("View not found"))?;
            let turn = turns
                .get(at_turn_id.as_str())
                .ok_or_else(|| anyhow::anyhow!("Turn not found"))?;
            (view.conversation_id.clone(), turn.sequence_number)
        };

        let new_view = ViewInfo {
            id: ViewId::new(),
            conversation_id: conversation_id.clone(),
            name: name.map(|s| s.to_string()),
            is_main: false,
            forked_from_view_id: Some(view_id.clone()),
            forked_at_turn_id: Some(at_turn_id.clone()),
            created_at: now(),
        };

        // Copy selections before the fork point
        {
            let turns = self.turns.lock().unwrap();
            let selections = self.view_selections.lock().unwrap();
            let mut new_selections = Vec::new();

            for ((vid, tid), sid) in selections.iter() {
                if vid == view_id.as_str() {
                    if let Some(turn) = turns.get(tid) {
                        if turn.sequence_number < at_turn_seq {
                            new_selections.push((tid.clone(), sid.clone()));
                        }
                    }
                }
            }

            drop(selections);
            let mut selections = self.view_selections.lock().unwrap();
            for (tid, sid) in new_selections {
                selections.insert(
                    (new_view.id.as_str().to_string(), tid),
                    sid,
                );
            }
        }

        let mut views = self.views.lock().unwrap();
        views.insert(new_view.id.as_str().to_string(), new_view.clone());

        Ok(new_view)
    }

    async fn fork_view_with_selections(
        &self,
        view_id: &ViewId,
        at_turn_id: &TurnId,
        name: Option<&str>,
        selections: &[(TurnId, SpanId)],
    ) -> Result<ViewInfo> {
        let new_view = self.fork_view(view_id, at_turn_id, name).await?;

        // Apply custom selections
        for (turn_id, span_id) in selections {
            self.select_span(&new_view.id, turn_id, span_id).await?;
        }

        Ok(new_view)
    }

    async fn get_view_context_at(
        &self,
        view_id: &ViewId,
        up_to_turn_id: &TurnId,
    ) -> Result<Vec<TurnWithContent>> {
        let (conversation_id, up_to_seq) = {
            let views = self.views.lock().unwrap();
            let turns = self.turns.lock().unwrap();
            let view = views
                .get(view_id.as_str())
                .ok_or_else(|| anyhow::anyhow!("View not found"))?;
            let turn = turns
                .get(up_to_turn_id.as_str())
                .ok_or_else(|| anyhow::anyhow!("Turn not found"))?;
            (view.conversation_id.clone(), turn.sequence_number)
        };

        let all_turns = self.get_turns(&conversation_id).await?;
        let turns: Vec<_> = all_turns
            .into_iter()
            .filter(|t| t.sequence_number < up_to_seq)
            .collect();

        let mut result = Vec::new();
        for turn in turns {
            let selected_span_id = self.get_selected_span(view_id, &turn.id).await?;

            let span = if let Some(span_id) = selected_span_id {
                self.get_span(&span_id).await?
            } else {
                let spans = self.get_spans(&turn.id).await?;
                spans.into_iter().next()
            };

            if let Some(span) = span {
                let messages = self.get_messages_with_content(&span.id).await?;
                result.push(TurnWithContent {
                    turn,
                    span,
                    messages,
                });
            }
        }

        Ok(result)
    }

    async fn edit_turn(
        &self,
        view_id: &ViewId,
        turn_id: &TurnId,
        messages: Vec<(MessageRole, Vec<StoredContent>)>,
        model_id: Option<&str>,
        create_fork: bool,
        fork_name: Option<&str>,
    ) -> Result<(SpanInfo, Option<ViewInfo>)> {
        let span = self.add_span(turn_id, model_id).await?;

        for (role, content) in messages {
            self.add_message(&span.id, role, &content).await?;
        }

        let forked_view = if create_fork {
            let new_view = self.fork_view(view_id, turn_id, fork_name).await?;
            self.select_span(&new_view.id, turn_id, &span.id).await?;
            Some(new_view)
        } else {
            self.select_span(view_id, turn_id, &span.id).await?;
            None
        };

        let span = self.get_span(&span.id).await?.unwrap_or(span);

        Ok((span, forked_view))
    }

    // ========== Convenience Methods ==========

    async fn add_user_turn(
        &self,
        conversation_id: &ConversationId,
        text: &str,
    ) -> Result<(TurnInfo, SpanInfo, MessageInfo)> {
        // Note: In a real implementation, this would store the text content.
        // For the memory implementation, we create a placeholder content block ID.
        let content_block_id = crate::storage::ids::ContentBlockId::new();

        let turn = self.add_turn(conversation_id, SpanRole::User).await?;
        let span = self.add_span(&turn.id, None).await?;
        let content = vec![StoredContent::text_ref(content_block_id)];
        let message = self.add_message(&span.id, MessageRole::User, &content).await?;

        if let Some(main_view) = self.get_main_view(conversation_id).await? {
            self.select_span(&main_view.id, &turn.id, &span.id).await?;
        }

        Ok((turn, span, message))
    }

    async fn add_assistant_turn(
        &self,
        conversation_id: &ConversationId,
        model_id: &str,
        text: &str,
    ) -> Result<(TurnInfo, SpanInfo, MessageInfo)> {
        // Note: In a real implementation, this would store the text content.
        // For the memory implementation, we create a placeholder content block ID.
        let content_block_id = crate::storage::ids::ContentBlockId::new();

        let turn = self.add_turn(conversation_id, SpanRole::Assistant).await?;
        let span = self.add_span(&turn.id, Some(model_id)).await?;
        let content = vec![StoredContent::text_ref(content_block_id)];
        let message = self
            .add_message(&span.id, MessageRole::Assistant, &content)
            .await?;

        if let Some(main_view) = self.get_main_view(conversation_id).await? {
            self.select_span(&main_view.id, &turn.id, &span.id).await?;
        }

        Ok((turn, span, message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_turn_crud() {
        let store = MemoryTurnStore::new();
        let conv_id = ConversationId::new();

        // Create view first
        let view = store
            .create_view(&conv_id, Some("main"), true)
            .await
            .unwrap();

        // Add user turn
        let turn1 = store.add_turn(&conv_id, SpanRole::User).await.unwrap();
        assert_eq!(turn1.sequence_number, 0);
        assert_eq!(turn1.role, SpanRole::User);

        // Add assistant turn
        let turn2 = store.add_turn(&conv_id, SpanRole::Assistant).await.unwrap();
        assert_eq!(turn2.sequence_number, 1);
        assert_eq!(turn2.role, SpanRole::Assistant);

        // Get turns
        let turns = store.get_turns(&conv_id).await.unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].id, turn1.id);
        assert_eq!(turns[1].id, turn2.id);
    }

    #[tokio::test]
    async fn test_span_and_message() {
        let store = MemoryTurnStore::new();
        let conv_id = ConversationId::new();

        // Create view and turn
        let _view = store
            .create_view(&conv_id, Some("main"), true)
            .await
            .unwrap();
        let turn = store.add_turn(&conv_id, SpanRole::User).await.unwrap();

        // Add span
        let span = store.add_span(&turn.id, None).await.unwrap();
        assert_eq!(span.message_count, 0);

        // Add message
        let content_block_id = crate::storage::ids::ContentBlockId::new();
        let content = vec![StoredContent::text_ref(content_block_id)];
        let _message = store
            .add_message(&span.id, MessageRole::User, &content)
            .await
            .unwrap();

        // Verify message
        let messages = store.get_messages_with_content(&span.id).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message.role, MessageRole::User);
        assert_eq!(messages[0].content.len(), 1);

        // Check span message count updated
        let span = store.get_span(&span.id).await.unwrap().unwrap();
        assert_eq!(span.message_count, 1);
    }

    #[tokio::test]
    async fn test_view_path() {
        let store = MemoryTurnStore::new();
        let conv_id = ConversationId::new();

        // Create main view
        let view = store
            .create_view(&conv_id, Some("main"), true)
            .await
            .unwrap();

        // Add user turn with message
        let (_turn1, _span1, _) = store.add_user_turn(&conv_id, "Hello").await.unwrap();

        // Add assistant turn with message
        let (_turn2, _span2, _) = store
            .add_assistant_turn(&conv_id, "claude", "Hi there!")
            .await
            .unwrap();

        // Get view path
        let path = store.get_view_path(&view.id).await.unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[0].turn.role, SpanRole::User);
        assert_eq!(path[1].turn.role, SpanRole::Assistant);
    }
}
