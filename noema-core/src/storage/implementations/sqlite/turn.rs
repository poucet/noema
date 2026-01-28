//! SQLite implementation of TurnStore

use anyhow::Result;
use async_trait::async_trait;
use llm::Role;
use rusqlite::{params, Connection};

use super::SqliteStore;
use crate::storage::content::StoredContent;
use crate::storage::helper::unix_timestamp;
use crate::storage::ids::{
    AssetId, ContentBlockId, ConversationId, DocumentId, MessageContentId, MessageId, SpanId, TurnId,
};
use crate::storage::traits::{StoredMessage, StoredSpan, StoredTurn, TurnStore};
use crate::storage::types::{stored, Message, MessageWithContent, Span, Turn, TurnWithContent};

/// Initialize turn-related schema (turns, spans, messages, conversation_selections)
pub(crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Turns: structural nodes that can have multiple spans
        -- Order is determined by conversation_selections.sequence_number
        CREATE TABLE IF NOT EXISTS turns (
            id TEXT PRIMARY KEY,
            role TEXT CHECK(role IN ('user', 'assistant')) NOT NULL,
            created_at INTEGER NOT NULL
        );

        -- Spans: alternative responses at a turn
        CREATE TABLE IF NOT EXISTS spans (
            id TEXT PRIMARY KEY,
            turn_id TEXT NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
            model_id TEXT,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_spans_turn ON spans(turn_id);

        -- Messages: individual messages within a span
        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            span_id TEXT NOT NULL REFERENCES spans(id) ON DELETE CASCADE,
            sequence_number INTEGER NOT NULL,
            role TEXT CHECK(role IN ('user', 'assistant', 'system', 'tool')) NOT NULL,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_messages_span ON messages(span_id, sequence_number);

        -- Message content: individual content items within a message
        CREATE TABLE IF NOT EXISTS message_content (
            id TEXT PRIMARY KEY,
            message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
            sequence_number INTEGER NOT NULL,
            content_type TEXT CHECK(content_type IN ('text', 'asset_ref', 'document_ref', 'tool_call', 'tool_result')) NOT NULL,
            -- For text: reference to content_blocks
            content_block_id TEXT REFERENCES content_blocks(id),
            -- For asset_ref: reference to blob storage
            asset_id TEXT,
            mime_type TEXT,
            -- For document_ref: reference to document
            document_id TEXT,
            -- For tool_call/tool_result: structured JSON
            tool_data TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_message_content_message ON message_content(message_id, sequence_number);
        CREATE INDEX IF NOT EXISTS idx_message_content_block ON message_content(content_block_id);

        -- Conversation selections: which span is selected at each turn for a conversation
        -- Each conversation has its own linear sequence of turns.
        -- sequence_number defines the order of turns within the conversation.
        CREATE TABLE IF NOT EXISTS conversation_selections (
            conversation_id TEXT NOT NULL,
            turn_id TEXT NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
            span_id TEXT NOT NULL REFERENCES spans(id) ON DELETE CASCADE,
            sequence_number INTEGER NOT NULL,
            PRIMARY KEY (conversation_id, turn_id)
        );
        CREATE INDEX IF NOT EXISTS idx_conv_selections_span ON conversation_selections(span_id);
        CREATE INDEX IF NOT EXISTS idx_conv_selections_seq ON conversation_selections(conversation_id, sequence_number);
        "#,
    )?;
    Ok(())
}

// ============================================================================
// Helper: Load message content from DB
// ============================================================================

fn load_message_content(
    conn: &Connection,
    message_id: &MessageId,
) -> Result<Vec<StoredContent>> {
    let mut stmt = conn.prepare(
        "SELECT content_type, content_block_id, asset_id, mime_type, document_id, tool_data
         FROM message_content WHERE message_id = ?1
         ORDER BY sequence_number",
    )?;

    let content = stmt
        .query_map(params![message_id.as_str()], |row| {
            let content_type: String = row.get(0)?;
            let content_block_id: Option<ContentBlockId> = row.get(1)?;
            let asset_id: Option<AssetId> = row.get(2)?;
            let mime_type: Option<String> = row.get(3)?;
            let document_id: Option<DocumentId> = row.get(4)?;
            let tool_data: Option<String> = row.get(5)?;
            Ok((content_type, content_block_id, asset_id, mime_type, document_id, tool_data))
        })?
        .filter_map(|r| r.ok())
        .filter_map(|(content_type, content_block_id, asset_id, mime_type, document_id, tool_data)| {
            match content_type.as_str() {
                "text" => Some(StoredContent::TextRef { content_block_id: content_block_id? }),
                "asset_ref" => Some(StoredContent::AssetRef {
                    asset_id: asset_id?,
                    mime_type: mime_type?,
                }),
                "document_ref" => Some(StoredContent::DocumentRef { document_id: document_id? }),
                "tool_call" => {
                    let call: llm::ToolCall = serde_json::from_str(&tool_data?).ok()?;
                    Some(StoredContent::ToolCall(call))
                }
                "tool_result" => {
                    let result: llm::ToolResult = serde_json::from_str(&tool_data?).ok()?;
                    Some(StoredContent::ToolResult(result))
                }
                _ => None,
            }
        })
        .collect();

    Ok(content)
}

// ============================================================================
// TurnStore Implementation
// ============================================================================

#[async_trait]
impl TurnStore for SqliteStore {
    // ========== Turn Management ==========

    async fn create_turn(&self, role: Role) -> Result<StoredTurn> {
        let conn = self.conn().lock().unwrap();
        let turn_id = TurnId::new();
        let now = unix_timestamp();

        conn.execute(
            "INSERT INTO turns (id, role, created_at) VALUES (?1, ?2, ?3)",
            params![turn_id.as_str(), role.as_str(), now],
        )?;

        Ok(stored(turn_id, Turn::new(role), now))
    }

    async fn get_turn(&self, turn_id: &TurnId) -> Result<Option<StoredTurn>> {
        let conn = self.conn().lock().unwrap();
        let result = conn.query_row(
            "SELECT id, role, created_at FROM turns WHERE id = ?1",
            params![turn_id.as_str()],
            |row| {
                let id: TurnId = row.get(0)?;
                let role_str: String = row.get(1)?;
                let created: i64 = row.get(2)?;
                Ok((id, role_str, created))
            },
        );

        match result {
            Ok((id, role_str, created)) => {
                let role = role_str
                    .parse::<Role>()
                    .map_err(|_| anyhow::anyhow!("Invalid role: {}", role_str))?;
                Ok(Some(stored(id, Turn::new(role), created)))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    // ========== Span Management ==========

    async fn create_span(&self, turn_id: &TurnId, model_id: Option<&str>) -> Result<StoredSpan> {
        let conn = self.conn().lock().unwrap();
        let id = SpanId::new();
        let now = unix_timestamp();

        conn.execute(
            "INSERT INTO spans (id, turn_id, model_id, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, turn_id, model_id, now],
        )?;

        let span = Span::new(model_id.map(|s| s.to_string()), 0);
        Ok(stored(id, span, now))
    }

    async fn get_spans(&self, turn_id: &TurnId) -> Result<Vec<StoredSpan>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT s.id, s.model_id, s.created_at,
                    (SELECT COUNT(*) FROM messages m WHERE m.span_id = s.id) as message_count
             FROM spans s WHERE s.turn_id = ?1
             ORDER BY s.created_at",
        )?;

        let spans = stmt
            .query_map(params![turn_id], |row| {
                let id: SpanId = row.get(0)?;
                let model: Option<String> = row.get(1)?;
                let created: i64 = row.get(2)?;
                let msg_count: i32 = row.get(3)?;
                Ok((id, model, created, msg_count))
            })?
            .filter_map(|r| r.ok())
            .map(|(id, model, created, msg_count)| {
                let span = Span::new(model, msg_count);
                stored(id, span, created)
            })
            .collect();

        Ok(spans)
    }

    async fn get_span(&self, span_id: &SpanId) -> Result<Option<StoredSpan>> {
        let conn = self.conn().lock().unwrap();
        let result = conn.query_row(
            "SELECT s.id, s.model_id, s.created_at,
                    (SELECT COUNT(*) FROM messages m WHERE m.span_id = s.id) as message_count
             FROM spans s WHERE s.id = ?1",
            params![span_id],
            |row| {
                let id: SpanId = row.get(0)?;
                let model: Option<String> = row.get(1)?;
                let created: i64 = row.get(2)?;
                let msg_count: i32 = row.get(3)?;
                Ok((id, model, created, msg_count))
            },
        );

        match result {
            Ok((id, model, created, msg_count)) => {
                let span = Span::new(model, msg_count);
                Ok(Some(stored(id, span, created)))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    // ========== Message Management ==========

    async fn add_message(
        &self,
        span_id: &SpanId,
        role: Role,
        content: &[StoredContent],
    ) -> Result<StoredMessage> {
        let conn = self.conn().lock().unwrap();
        let message_id = MessageId::new();
        let now = unix_timestamp();

        // Get next sequence number for message
        let sequence_number: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(sequence_number), -1) + 1 FROM messages WHERE span_id = ?1",
                params![span_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Insert message row
        conn.execute(
            "INSERT INTO messages (id, span_id, sequence_number, role, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                message_id,
                span_id,
                sequence_number,
                role.as_str(),
                now
            ],
        )?;

        // Insert content items
        insert_message_content(&conn, &message_id, content)?;

        let message = Message::new(span_id.clone(), sequence_number, role);
        Ok(stored(message_id, message, now))
    }

    async fn get_messages(&self, span_id: &SpanId) -> Result<Vec<MessageWithContent>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, span_id, sequence_number, role, created_at
             FROM messages WHERE span_id = ?1
             ORDER BY sequence_number",
        )?;

        let messages: Vec<StoredMessage> = stmt
            .query_map(params![span_id], |row| {
                let id: MessageId = row.get(0)?;
                let sid: SpanId = row.get(1)?;
                let seq: i32 = row.get(2)?;
                let role_str: String = row.get(3)?;
                let created: i64 = row.get(4)?;
                Ok((id, sid, seq, role_str, created))
            })?
            .filter_map(|r| r.ok())
            .filter_map(|(id, sid, seq, role_str, created)| {
                let role = role_str.parse::<Role>().ok()?;
                let message = Message::new(sid, seq, role);
                Some(stored(id, message, created))
            })
            .collect();

        let mut result = Vec::new();
        for message in messages {
            let content = load_message_content(&conn, &message.id)?;
            result.push(MessageWithContent { message, content });
        }

        Ok(result)
    }

    async fn get_message(&self, message_id: &MessageId) -> Result<Option<StoredMessage>> {
        let conn = self.conn().lock().unwrap();
        let result = conn.query_row(
            "SELECT id, span_id, sequence_number, role, created_at
             FROM messages WHERE id = ?1",
            params![message_id],
            |row| {
                let id: MessageId = row.get(0)?;
                let sid: SpanId = row.get(1)?;
                let seq: i32 = row.get(2)?;
                let role_str: String = row.get(3)?;
                let created: i64 = row.get(4)?;
                Ok((id, sid, seq, role_str, created))
            },
        );

        match result {
            Ok((id, sid, seq, role_str, created)) => {
                let role = role_str
                    .parse::<Role>()
                    .map_err(|_| anyhow::anyhow!("Invalid role: {}", role_str))?;
                let message = Message::new(sid, seq, role);
                Ok(Some(stored(id, message, created)))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    // ========== Selection Management ==========

    async fn select_span(
        &self,
        conversation_id: &ConversationId,
        turn_id: &TurnId,
        span_id: &SpanId,
    ) -> Result<()> {
        let conn = self.conn().lock().unwrap();

        // Get next sequence number for this conversation (only used for new insertions)
        let sequence_number: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(sequence_number), -1) + 1 FROM conversation_selections WHERE conversation_id = ?1",
                params![conversation_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        conn.execute(
            "INSERT INTO conversation_selections (conversation_id, turn_id, span_id, sequence_number)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(conversation_id, turn_id) DO UPDATE SET span_id = ?3",
            params![conversation_id, turn_id, span_id, sequence_number],
        )?;
        Ok(())
    }

    async fn get_selected_span(
        &self,
        conversation_id: &ConversationId,
        turn_id: &TurnId,
    ) -> Result<Option<SpanId>> {
        let conn = self.conn().lock().unwrap();
        let result = conn.query_row(
            "SELECT span_id FROM conversation_selections WHERE conversation_id = ?1 AND turn_id = ?2",
            params![conversation_id, turn_id],
            |row| {
                let span_id: SpanId = row.get(0)?;
                Ok(span_id)
            },
        );

        match result {
            Ok(span_id) => Ok(Some(span_id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn get_conversation_path(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Vec<TurnWithContent>> {
        // Get all selections for this conversation, ordered by sequence_number
        let selections: Vec<(TurnId, SpanId)> = {
            let conn = self.conn().lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT turn_id, span_id FROM conversation_selections
                 WHERE conversation_id = ?1
                 ORDER BY sequence_number",
            )?;

            let rows = stmt.query_map(params![conversation_id], |row| {
                let turn_id: TurnId = row.get(0)?;
                let span_id: SpanId = row.get(1)?;
                Ok((turn_id, span_id))
            })?;

            rows.filter_map(|r| r.ok()).collect()
        };

        if selections.is_empty() {
            return Ok(Vec::new());
        }

        let mut result = Vec::new();
        for (turn_id, span_id) in selections {
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
        // Get sequence number of the up_to turn in this conversation
        let up_to_seq: i32 = {
            let conn = self.conn().lock().unwrap();
            conn.query_row(
                "SELECT sequence_number FROM conversation_selections WHERE conversation_id = ?1 AND turn_id = ?2",
                params![conversation_id, up_to_turn_id],
                |row| row.get(0),
            )?
        };

        // Get all selections before the up_to turn
        let selections: Vec<(TurnId, SpanId)> = {
            let conn = self.conn().lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT turn_id, span_id FROM conversation_selections
                 WHERE conversation_id = ?1 AND sequence_number < ?2
                 ORDER BY sequence_number",
            )?;

            let rows = stmt.query_map(params![conversation_id, up_to_seq], |row| {
                let turn_id: TurnId = row.get(0)?;
                let span_id: SpanId = row.get(1)?;
                Ok((turn_id, span_id))
            })?;

            rows.filter_map(|r| r.ok()).collect()
        };

        let mut result = Vec::new();
        for (turn_id, span_id) in selections {
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
        let conn = self.conn().lock().unwrap();

        // Get the sequence number of the cutoff turn
        let cutoff_seq: i32 = conn.query_row(
            "SELECT sequence_number FROM conversation_selections WHERE conversation_id = ?1 AND turn_id = ?2",
            params![from_conversation_id, up_to_turn_id],
            |row| row.get(0),
        )?;

        // Copy selections: if include_turn is true, include the turn, otherwise exclude it
        let cutoff = if include_turn { cutoff_seq + 1 } else { cutoff_seq };

        let copied = conn.execute(
            "INSERT INTO conversation_selections (conversation_id, turn_id, span_id, sequence_number)
             SELECT ?1, cs.turn_id, cs.span_id, cs.sequence_number
             FROM conversation_selections cs
             WHERE cs.conversation_id = ?2
               AND cs.sequence_number < ?3",
            params![to_conversation_id, from_conversation_id, cutoff],
        )?;

        Ok(copied)
    }

    async fn get_turn_count(&self, conversation_id: &ConversationId) -> Result<usize> {
        let conn = self.conn().lock().unwrap();
        let count: usize = conn.query_row(
            "SELECT COUNT(*) FROM conversation_selections WHERE conversation_id = ?1",
            params![conversation_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}

// ============================================================================
// Helper: Insert message content
// ============================================================================

fn insert_message_content(
    conn: &Connection,
    message_id: &MessageId,
    content: &[StoredContent],
) -> Result<()> {
    for (content_seq, stored_content) in content.iter().enumerate() {
        let content_id = MessageContentId::new();

        match stored_content {
            StoredContent::TextRef { content_block_id } => {
                conn.execute(
                    "INSERT INTO message_content (id, message_id, sequence_number, content_type, content_block_id)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        content_id,
                        message_id,
                        content_seq as i32,
                        "text",
                        content_block_id
                    ],
                )?;
            }
            StoredContent::AssetRef {
                asset_id,
                mime_type,
            } => {
                conn.execute(
                    "INSERT INTO message_content (id, message_id, sequence_number, content_type, asset_id, mime_type)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        content_id,
                        message_id,
                        content_seq as i32,
                        "asset_ref",
                        asset_id,
                        mime_type                    ],
                )?;
            }
            StoredContent::DocumentRef { document_id } => {
                conn.execute(
                    "INSERT INTO message_content (id, message_id, sequence_number, content_type, document_id)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        content_id,
                        message_id,
                        content_seq as i32,
                        "document_ref",
                        document_id
                    ],
                )?;
            }
            StoredContent::ToolCall(call) => {
                let tool_data = serde_json::to_string(call)?;
                conn.execute(
                    "INSERT INTO message_content (id, message_id, sequence_number, content_type, tool_data)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        content_id,
                        message_id,
                        content_seq as i32,
                        "tool_call",
                        tool_data
                    ],
                )?;
            }
            StoredContent::ToolResult(result) => {
                let tool_data = serde_json::to_string(result)?;
                conn.execute(
                    "INSERT INTO message_content (id, message_id, sequence_number, content_type, tool_data)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        content_id,
                        message_id,
                        content_seq as i32,
                        "tool_result",
                        tool_data
                    ],
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use llm::Role;

    use super::*;

    fn create_test_store() -> SqliteStore {
        SqliteStore::in_memory().unwrap()
    }

    #[tokio::test]
    async fn test_turn_crud() {
        let store = create_test_store();

        // Create user turn
        let turn1 = store.create_turn(llm::Role::User).await.unwrap();
        assert_eq!(turn1.role(), Role::User);

        // Create assistant turn
        let turn2 = store.create_turn(llm::Role::Assistant).await.unwrap();
        assert_eq!(turn2.role(), Role::Assistant);
        // Get turns individually
        let fetched = store.get_turn(&turn1.id).await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id, turn1.id);
    }

    #[tokio::test]
    async fn test_span_and_message() {
        let store = create_test_store();

        // Create turn
        let turn = store.create_turn(llm::Role::User).await.unwrap();

        // Create span
        let span = store.create_span(&turn.id, None).await.unwrap();
        assert_eq!(span.message_count, 0);

        // Add message
        let content_block_id = ContentBlockId::new();
        let content = vec![StoredContent::text_ref(content_block_id)];
        let _message = store
            .add_message(&span.id, Role::User, &content)
            .await
            .unwrap();

        // Verify message (get_messages returns MessageWithContent)
        let messages = store.get_messages(&span.id).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message.role, Role::User);
        assert_eq!(messages[0].content.len(), 1);

        // Check span message count updated
        let span = store.get_span(&span.id).await.unwrap().unwrap();
        assert_eq!(span.message_count, 1);
    }

    #[tokio::test]
    async fn test_conversation_path() {
        let store = create_test_store();

        // Use a conversation ID (in real usage, this would come from EntityStore)
        let conversation_id = ConversationId::new();

        // Create user turn with span and message, select in conversation
        let turn1 = store.create_turn(llm::Role::User).await.unwrap();
        let span1 = store.create_span(&turn1.id, None).await.unwrap();
        let content_block_id = ContentBlockId::new();
        let content = vec![StoredContent::text_ref(content_block_id)];
        store.add_message(&span1.id, Role::User, &content).await.unwrap();
        store.select_span(&conversation_id, &turn1.id, &span1.id).await.unwrap();

        // Create assistant turn with span and message, select in conversation
        let turn2 = store.create_turn(llm::Role::Assistant).await.unwrap();
        let span2 = store.create_span(&turn2.id, Some("claude")).await.unwrap();
        let content_block_id2 = ContentBlockId::new();
        let content2 = vec![StoredContent::text_ref(content_block_id2)];
        store.add_message(&span2.id, Role::Assistant, &content2).await.unwrap();
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
        let store = create_test_store();

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
