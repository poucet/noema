//! In-memory TurnStore implementation

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::storage::content::StoredContent;
use crate::storage::ids::{MessageId, SpanId, TurnId, ViewId};
use crate::storage::traits::TurnStore;
use crate::storage::types::{
    stored, ForkInfo, Message, MessageRole, MessageWithContent, Span, Stored, Turn,
    TurnWithContent, View,
};

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// View selection with sequence number
#[derive(Debug, Clone)]
struct ViewSelection {
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
    views: Mutex<HashMap<ViewId, Stored<ViewId, View>>>,
    view_selections: Mutex<HashMap<(ViewId, TurnId), ViewSelection>>,
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
        role: MessageRole,
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

    // ========== View Management ==========

    async fn create_view(&self) -> Result<Stored<ViewId, View>> {
        let mut views = self.views.lock().unwrap();

        let id = ViewId::new();
        let now = now();
        let view = View {
            fork: None,
            turn_count: 0,
        };
        let stored = stored(id.clone(), view, now);

        views.insert(id, stored.clone());
        Ok(stored)
    }

    async fn get_view(&self, view_id: &ViewId) -> Result<Option<Stored<ViewId, View>>> {
        let views = self.views.lock().unwrap();
        let selections = self.view_selections.lock().unwrap();

        Ok(views.get(view_id).map(|v| {
            let turn_count = selections
                .iter()
                .filter(|((vid, _), _)| vid == view_id)
                .count();
            let view = View {
                turn_count,
                fork: v.fork.clone(),
            };
            stored(v.id.clone(), view, v.created_at)
        }))
    }

    async fn list_related_views(&self, main_view_id: &ViewId) -> Result<Vec<Stored<ViewId, View>>> {
        let views = self.views.lock().unwrap();
        let selections = self.view_selections.lock().unwrap();

        // Collect all views that are part of the fork tree starting from main_view_id
        let mut result = Vec::new();
        let mut to_visit = vec![main_view_id.clone()];
        let mut visited = std::collections::HashSet::new();

        while let Some(vid) = to_visit.pop() {
            if visited.contains(&vid) {
                continue;
            }
            visited.insert(vid.clone());

            if let Some(v) = views.get(&vid) {
                let turn_count = selections
                    .iter()
                    .filter(|((view_id, _), _)| view_id == &vid)
                    .count();
                let view = View {
                    turn_count,
                    fork: v.fork.clone(),
                };
                result.push(stored(v.id.clone(), view, v.created_at));

                // Find all views that were forked from this view
                for (other_id, other_view) in views.iter() {
                    if let Some(ref fork) = other_view.fork {
                        if &fork.from_view_id == &vid {
                            to_visit.push(other_id.clone());
                        }
                    }
                }
            }
        }

        // Sort by created_at
        result.sort_by_key(|v| v.created_at);
        Ok(result)
    }

    async fn select_span(
        &self,
        view_id: &ViewId,
        turn_id: &TurnId,
        span_id: &SpanId,
    ) -> Result<()> {
        let mut selections = self.view_selections.lock().unwrap();

        // Get next sequence number for this view
        let sequence_number = selections
            .iter()
            .filter(|((vid, _), _)| vid == view_id)
            .map(|(_, sel)| sel.sequence_number)
            .max()
            .map(|n| n + 1)
            .unwrap_or(0);

        selections.insert(
            (view_id.clone(), turn_id.clone()),
            ViewSelection { span_id: span_id.clone(), sequence_number },
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
            .get(&(view_id.clone(), turn_id.clone()))
            .map(|sel| sel.span_id.clone()))
    }

    async fn get_view_path(&self, view_id: &ViewId) -> Result<Vec<TurnWithContent>> {
        // Get all selections for this view, sorted by sequence
        let selections: Vec<_> = {
            let sels = self.view_selections.lock().unwrap();
            let mut entries: Vec<_> = sels
                .iter()
                .filter(|((vid, _), _)| vid == view_id)
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

    async fn fork_view(&self, view_id: &ViewId, at_turn_id: &TurnId) -> Result<Stored<ViewId, View>> {
        // Find the sequence number of the fork point
        let at_turn_seq = {
            let selections = self.view_selections.lock().unwrap();
            selections
                .get(&(view_id.clone(), at_turn_id.clone()))
                .map(|sel| sel.sequence_number)
                .ok_or_else(|| anyhow::anyhow!("Turn not in view"))?
        };

        // Copy selections before the fork point and count them
        let new_selections: Vec<_> = {
            let selections = self.view_selections.lock().unwrap();
            selections
                .iter()
                .filter(|((vid, _), sel)| vid == view_id && sel.sequence_number < at_turn_seq)
                .map(|((_, tid), sel)| (tid.clone(), sel.clone()))
                .collect()
        };

        let turn_count = new_selections.len();

        let id = ViewId::new();
        let now = now();
        let view = View {
            fork: Some(ForkInfo {
                from_view_id: view_id.clone(),
                at_turn_id: at_turn_id.clone(),
            }),
            turn_count,
        };
        let stored = stored(id.clone(), view, now);

        {
            let mut selections = self.view_selections.lock().unwrap();
            for (tid, sel) in new_selections {
                selections.insert((id.clone(), tid), sel);
            }
        }

        let mut views = self.views.lock().unwrap();
        views.insert(id, stored.clone());

        Ok(stored)
    }

    async fn get_view_context_at(
        &self,
        view_id: &ViewId,
        up_to_turn_id: &TurnId,
    ) -> Result<Vec<TurnWithContent>> {
        // Get sequence of the up_to turn in this view
        let up_to_seq = {
            let selections = self.view_selections.lock().unwrap();
            selections
                .get(&(view_id.clone(), up_to_turn_id.clone()))
                .map(|sel| sel.sequence_number)
                .ok_or_else(|| anyhow::anyhow!("Turn not in view"))?
        };

        // Get all selections before the up_to turn
        let selections: Vec<_> = {
            let sels = self.view_selections.lock().unwrap();
            let mut entries: Vec<_> = sels
                .iter()
                .filter(|((vid, _), sel)| vid == view_id && sel.sequence_number < up_to_seq)
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

    async fn edit_turn(
        &self,
        view_id: &ViewId,
        turn_id: &TurnId,
        messages: Vec<(MessageRole, Vec<StoredContent>)>,
        model_id: Option<&str>,
        create_fork: bool,
    ) -> Result<(Stored<SpanId, Span>, Option<Stored<ViewId, View>>)> {
        let span = self.create_span(turn_id, model_id).await?;

        for (role, content) in messages {
            self.add_message(&span.id, role, &content).await?;
        }

        let forked_view = if create_fork {
            let new_view = self.fork_view(view_id, turn_id).await?;
            self.select_span(&new_view.id, turn_id, &span.id).await?;
            Some(new_view)
        } else {
            self.select_span(view_id, turn_id, &span.id).await?;
            None
        };

        let span = self.get_span(&span.id).await?.unwrap_or(span);

        Ok((span, forked_view))
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
            .add_message(&span.id, MessageRole::User, &content)
            .await
            .unwrap();

        // Verify message (get_messages returns MessageWithContent)
        let messages = store.get_messages(&span.id).await.unwrap();
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

        // Create view
        let view = store.create_view().await.unwrap();

        // Create user turn with span and message, select in view
        let turn1 = store.create_turn(llm::Role::User).await.unwrap();
        let span1 = store.create_span(&turn1.id, None).await.unwrap();
        let content_block_id = crate::storage::ids::ContentBlockId::new();
        let content = vec![StoredContent::text_ref(content_block_id)];
        store.add_message(&span1.id, MessageRole::User, &content).await.unwrap();
        store.select_span(&view.id, &turn1.id, &span1.id).await.unwrap();

        // Create assistant turn with span and message, select in view
        let turn2 = store.create_turn(llm::Role::Assistant).await.unwrap();
        let span2 = store.create_span(&turn2.id, Some("claude")).await.unwrap();
        let content_block_id2 = crate::storage::ids::ContentBlockId::new();
        let content2 = vec![StoredContent::text_ref(content_block_id2)];
        store.add_message(&span2.id, MessageRole::Assistant, &content2).await.unwrap();
        store.select_span(&view.id, &turn2.id, &span2.id).await.unwrap();

        // Get view path
        let path = store.get_view_path(&view.id).await.unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[0].turn.role(), llm::Role::User);
        assert_eq!(path[1].turn.role(), llm::Role::Assistant);
    }
}
