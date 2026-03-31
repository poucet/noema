//! In-memory TurnStore implementation

use anyhow::Result;
use async_trait::async_trait;
use llm::Role;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::storage::content::StoredContent;
use crate::storage::ids::{ConversationId, MessageId, SpanId, TurnId};
use crate::storage::traits::TurnStore;
use crate::storage::types::{stored, Message, MessageWithContent, Span, Stored, Turn, TurnWithContent};

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Conversation selection with sequence number
#[derive(Debug, Clone)]
struct ConversationSelection {
    span_id: SpanId,
    sequence_number: i32,
}

/// Span with parent turn tracking (internal only)
#[derive(Debug, Clone)]
struct InternalSpan {
    span: Stored<SpanId, Span>,
    turn_id: TurnId,
}

/// In-memory turn store for testing
#[derive(Debug, Default)]
pub struct MemoryTurnStore {
    turns: Mutex<HashMap<TurnId, Stored<TurnId, Turn>>>,
    spans: Mutex<HashMap<SpanId, InternalSpan>>,
    messages: Mutex<HashMap<MessageId, Stored<MessageId, Message>>>,
    message_content: Mutex<HashMap<MessageId, Vec<StoredContent>>>,
    conversation_selections: Mutex<HashMap<(ConversationId, TurnId), ConversationSelection>>,
}

impl MemoryTurnStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl TurnStore for MemoryTurnStore {
    // ========== Turn Management ==========

    async fn create_turn(&self, role: llm::Role) -> Result<Stored<TurnId, Turn>> {
        let mut turns = self.turns.lock().unwrap();

        let id = TurnId::new();
        let now = now();
        let turn = stored(id.clone(), Turn::new(role), now);

        turns.insert(id, turn.clone());
        Ok(turn)
    }

    async fn get_turn(&self, turn_id: &TurnId) -> Result<Option<Stored<TurnId, Turn>>> {
        let turns = self.turns.lock().unwrap();
        Ok(turns.get(turn_id).cloned())
    }

    // ========== Span Management ==========

    async fn create_span(&self, turn_id: &TurnId, model_id: Option<&str>) -> Result<Stored<SpanId, Span>> {
        let mut spans = self.spans.lock().unwrap();

        let id = SpanId::new();
        let now = now();
        let span = Span {
            model_id: model_id.map(|s| s.to_string()),
            message_count: 0,
        };
        let stored = stored(id.clone(), span, now);

        spans.insert(id, InternalSpan {
            span: stored.clone(),
            turn_id: turn_id.clone(),
        });
        Ok(stored)
    }

    async fn get_spans(&self, turn_id: &TurnId) -> Result<Vec<Stored<SpanId, Span>>> {
        let spans = self.spans.lock().unwrap();
        let messages = self.messages.lock().unwrap();

        let mut result: Vec<_> = spans
            .values()
            .filter(|s| s.turn_id == *turn_id)
            .map(|s| {
                let message_count = messages
                    .values()
                    .filter(|m| m.span_id == s.span.id)
                    .count() as i32;
                let span = Span {
                    message_count,
                    model_id: s.span.model_id.clone(),
                };
                stored(s.span.id.clone(), span, s.span.created_at)
            })
            .collect();
        result.sort_by_key(|s| s.created_at);
        Ok(result)
    }

    async fn get_span(&self, span_id: &SpanId) -> Result<Option<Stored<SpanId, Span>>> {
        let spans = self.spans.lock().unwrap();
        let messages = self.messages.lock().unwrap();

        Ok(spans.get(span_id).map(|s| {
            let message_count = messages
                .values()
                .filter(|m| m.span_id == s.span.id)
                .count() as i32;
            let span = Span {
                message_count,
                model_id: s.span.model_id.clone(),
            };
            stored(s.span.id.clone(), span, s.span.created_at)
        }))
    }

    // ========== Message Management ==========

    async fn add_message(
        &self,
        span_id: &SpanId,
        role: Role,
        content: &[StoredContent],
    ) -> Result<Stored<MessageId, Message>> {
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
        let now = now();
        let msg = Message {
            span_id: span_id.clone(),
            sequence_number,
            role,
        };
        let stored = stored(message_id.clone(), msg, now);

        messages.insert(message_id.clone(), stored.clone());
        message_content_map.insert(message_id, content.to_vec());

        Ok(stored)
    }

    async fn get_messages(&self, span_id: &SpanId) -> Result<Vec<MessageWithContent>> {
        let messages = self.messages.lock().unwrap();
        let message_content_map = self.message_content.lock().unwrap();

        let mut message_list: Vec<_> = messages
            .values()
            .filter(|m| m.span_id == *span_id)
            .cloned()
            .collect();
        message_list.sort_by_key(|m| m.sequence_number);

        let result = message_list
            .into_iter()
            .map(|message| {
                let content = message_content_map
                    .get(&message.id)
                    .cloned()
                    .unwrap_or_default();
                MessageWithContent { message, content }
            })
            .collect();

        Ok(result)
    }

    async fn get_message(&self, message_id: &MessageId) -> Result<Option<Stored<MessageId, Message>>> {
        let messages = self.messages.lock().unwrap();
        Ok(messages.get(message_id).cloned())
    }

    // ========== Selection Management ==========

    async fn select_span(
        &self,
        conversation_id: &ConversationId,
        turn_id: &TurnId,
        span_id: &SpanId,
    ) -> Result<()> {
        let mut selections = self.conversation_selections.lock().unwrap();

        // Get next sequence number for this conversation
        let sequence_number = selections
            .iter()
            .filter(|((cid, _), _)| cid == conversation_id)
            .map(|(_, sel)| sel.sequence_number)
            .max()
            .map(|n| n + 1)
            .unwrap_or(0);

        selections.insert(
            (conversation_id.clone(), turn_id.clone()),
            ConversationSelection {
                span_id: span_id.clone(),
                sequence_number,
            },
        );
        Ok(())
    }

    async fn get_selected_span(
        &self,
        conversation_id: &ConversationId,
        turn_id: &TurnId,
    ) -> Result<Option<SpanId>> {
        let selections = self.conversation_selections.lock().unwrap();
        Ok(selections
            .get(&(conversation_id.clone(), turn_id.clone()))
            .map(|sel| sel.span_id.clone()))
    }

    async fn get_conversation_path(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Vec<TurnWithContent>> {
        // Get all selections for this conversation, sorted by sequence
        let selections: Vec<_> = {
            let sels = self.conversation_selections.lock().unwrap();
            let mut entries: Vec<_> = sels
                .iter()
                .filter(|((cid, _), _)| cid == conversation_id)
                .map(|((_, tid), sel)| (tid.clone(), sel.span_id.clone(), sel.sequence_number))
                .collect();
            entries.sort_by_key(|(_, _, seq)| *seq);
            entries
        };

        if selections.is_empty() {
            return Ok(Vec::new());
        }

        let mut result = Vec::new();

        for (turn_id, span_id, _seq) in selections {
            let turn = self.get_turn(&turn_id).await?.ok_or_else(|| {
                anyhow::anyhow!("Turn not found: {}", turn_id)
            })?;
            let span = self.get_span(&span_id).await?.ok_or_else(|| {
                anyhow::anyhow!("Span not found: {}", span_id)
            })?;
            let messages = self.get_messages(&span_id).await?;

            result.push(TurnWithContent {
                turn,
                span,
                messages,
            });
        }

        Ok(result)
    }

    async fn get_context_at(
        &self,
        conversation_id: &ConversationId,
        up_to_turn_id: &TurnId,
    ) -> Result<Vec<TurnWithContent>> {
        // Get sequence of the up_to turn in this conversation
        let up_to_seq = {
            let selections = self.conversation_selections.lock().unwrap();
            selections
                .get(&(conversation_id.clone(), up_to_turn_id.clone()))
                .map(|sel| sel.sequence_number)
                .ok_or_else(|| anyhow::anyhow!("Turn not in conversation"))?
        };

        // Get all selections before the up_to turn
        let selections: Vec<_> = {
            let sels = self.conversation_selections.lock().unwrap();
            let mut entries: Vec<_> = sels
                .iter()
                .filter(|((cid, _), sel)| cid == conversation_id && sel.sequence_number < up_to_seq)
                .map(|((_, tid), sel)| (tid.clone(), sel.span_id.clone(), sel.sequence_number))
                .collect();
            entries.sort_by_key(|(_, _, seq)| *seq);
            entries
        };

        let mut result = Vec::new();
        for (turn_id, span_id, _seq) in selections {
            let turn = self.get_turn(&turn_id).await?.ok_or_else(|| {
                anyhow::anyhow!("Turn not found: {}", turn_id)
            })?;
            let span = self.get_span(&span_id).await?.ok_or_else(|| {
                anyhow::anyhow!("Span not found: {}", span_id)
            })?;
            let messages = self.get_messages(&span_id).await?;

            result.push(TurnWithContent {
                turn,
                span,
                messages,
            });
        }

        Ok(result)
    }

    async fn copy_selections(
        &self,
        from_conversation_id: &ConversationId,
        to_conversation_id: &ConversationId,
        up_to_turn_id: &TurnId,
        include_turn: bool,
    ) -> Result<usize> {
        // Find the sequence number of the cutoff turn
        let cutoff_seq = {
            let selections = self.conversation_selections.lock().unwrap();
            selections
                .get(&(from_conversation_id.clone(), up_to_turn_id.clone()))
                .map(|sel| sel.sequence_number)
                .ok_or_else(|| anyhow::anyhow!("Turn not in source conversation"))?
        };

        // Cutoff: if include_turn, include the turn, otherwise exclude
        let cutoff = if include_turn { cutoff_seq + 1 } else { cutoff_seq };

        // Get selections to copy
        let selections_to_copy: Vec<_> = {
            let selections = self.conversation_selections.lock().unwrap();
            selections
                .iter()
                .filter(|((cid, _), sel)| cid == from_conversation_id && sel.sequence_number < cutoff)
                .map(|((_, tid), sel)| (tid.clone(), sel.clone()))
                .collect()
        };

        let count = selections_to_copy.len();

        // Insert the copied selections
        {
            let mut selections = self.conversation_selections.lock().unwrap();
            for (tid, sel) in selections_to_copy {
                selections.insert((to_conversation_id.clone(), tid), sel);
            }
        }

        Ok(count)
    }

    async fn get_turn_count(&self, conversation_id: &ConversationId) -> Result<usize> {
        let selections = self.conversation_selections.lock().unwrap();
        let count = selections
            .iter()
            .filter(|((cid, _), _)| cid == conversation_id)
            .count();
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_turn_crud() {
        let store = MemoryTurnStore::new();

        // Create user turn
        let turn1 = store.create_turn(llm::Role::User).await.unwrap();
        assert_eq!(turn1.role(), llm::Role::User);

        // Create assistant turn
        let turn2 = store.create_turn(llm::Role::Assistant).await.unwrap();
        assert_eq!(turn2.role(), llm::Role::Assistant);

        // Get turns individually
        let fetched = store.get_turn(&turn1.id).await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id, turn1.id);
    }

    #[tokio::test]
    async fn test_span_and_message() {
        let store = MemoryTurnStore::new();

        // Create turn
        let turn = store.create_turn(llm::Role::User).await.unwrap();

        // Create span
        let span = store.create_span(&turn.id, None).await.unwrap();
        assert_eq!(span.message_count, 0);

        // Add message
        let content_block_id = crate::storage::ids::ContentBlockId::new();
        let content = vec![StoredContent::text_ref(content_block_id)];
        let _message = store
            .add_message(&span.id, llm::Role::User, &content)
            .await
            .unwrap();

        // Verify message (get_messages returns MessageWithContent)
        let messages = store.get_messages(&span.id).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message.role, llm::Role::User);
        assert_eq!(messages[0].content.len(), 1);

        // Check span message count updated
        let span = store.get_span(&span.id).await.unwrap().unwrap();
        assert_eq!(span.message_count, 1);
    }

    #[tokio::test]
    async fn test_conversation_path() {
        let store = MemoryTurnStore::new();

        // Use a conversation ID
        let conversation_id = ConversationId::new();

        // Create user turn with span and message, select in conversation
        let turn1 = store.create_turn(llm::Role::User).await.unwrap();
        let span1 = store.create_span(&turn1.id, None).await.unwrap();
        let content_block_id = crate::storage::ids::ContentBlockId::new();
        let content = vec![StoredContent::text_ref(content_block_id)];
        store.add_message(&span1.id, llm::Role::User, &content).await.unwrap();
        store.select_span(&conversation_id, &turn1.id, &span1.id).await.unwrap();

        // Create assistant turn with span and message, select in conversation
        let turn2 = store.create_turn(llm::Role::Assistant).await.unwrap();
        let span2 = store.create_span(&turn2.id, Some("claude")).await.unwrap();
        let content_block_id2 = crate::storage::ids::ContentBlockId::new();
        let content2 = vec![StoredContent::text_ref(content_block_id2)];
        store.add_message(&span2.id, llm::Role::Assistant, &content2).await.unwrap();
        store.select_span(&conversation_id, &turn2.id, &span2.id).await.unwrap();

        // Get conversation path
        let path = store.get_conversation_path(&conversation_id).await.unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[0].turn.role(), llm::Role::User);
        assert_eq!(path[1].turn.role(), llm::Role::Assistant);

        // Verify turn count
        let count = store.get_turn_count(&conversation_id).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_copy_selections() {
        let store = MemoryTurnStore::new();

        let conv1 = ConversationId::new();
        let conv2 = ConversationId::new();

        // Create a conversation with 3 turns
        let turn1 = store.create_turn(llm::Role::User).await.unwrap();
        let span1 = store.create_span(&turn1.id, None).await.unwrap();
        store.select_span(&conv1, &turn1.id, &span1.id).await.unwrap();

        let turn2 = store.create_turn(llm::Role::Assistant).await.unwrap();
        let span2 = store.create_span(&turn2.id, Some("claude")).await.unwrap();
        store.select_span(&conv1, &turn2.id, &span2.id).await.unwrap();

        let turn3 = store.create_turn(llm::Role::User).await.unwrap();
        let span3 = store.create_span(&turn3.id, None).await.unwrap();
        store.select_span(&conv1, &turn3.id, &span3.id).await.unwrap();

        // Copy up to turn2 (include_turn = true) - should get turns 1 and 2
        let copied = store.copy_selections(&conv1, &conv2, &turn2.id, true).await.unwrap();
        assert_eq!(copied, 2);

        let path = store.get_conversation_path(&conv2).await.unwrap();
        assert_eq!(path.len(), 2);

        // Copy to another conv up to turn2 (include_turn = false) - should get only turn 1
        let conv3 = ConversationId::new();
        let copied = store.copy_selections(&conv1, &conv3, &turn2.id, false).await.unwrap();
        assert_eq!(copied, 1);
    }
}
