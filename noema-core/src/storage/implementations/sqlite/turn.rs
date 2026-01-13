//! SQLite implementation of TurnStore

use anyhow::Result;
use async_trait::async_trait;
use rusqlite::{params, Connection};

use super::SqliteStore;
use crate::storage::content::StoredContent;
use crate::storage::helper::unix_timestamp;
use crate::storage::ids::{
    AssetId, ContentBlockId, DocumentId, MessageContentId, MessageId, SpanId, TurnId, ViewId
};
use crate::storage::traits::TurnStore;
use crate::storage::types::{
    stored, ForkInfo, Message, MessageRole, MessageWithContent, Span, SpanRole,
    Stored, Turn, TurnWithContent, View,
};

/// Initialize turn-related schema (turns, spans, messages, views)
pub(crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Turns: structural nodes that can have multiple spans
        -- Order is determined by view_selections.sequence_number
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

        -- Views: paths through conversation (span selections per turn)
        CREATE TABLE IF NOT EXISTS views (
            id TEXT PRIMARY KEY,
            forked_from_view_id TEXT REFERENCES views(id),
            forked_at_turn_id TEXT REFERENCES turns(id),
            created_at INTEGER NOT NULL
        );

        -- View selections: which span is selected at each turn for a view
        -- sequence_number defines the order of turns within this view
        CREATE TABLE IF NOT EXISTS view_selections (
            view_id TEXT NOT NULL REFERENCES views(id) ON DELETE CASCADE,
            turn_id TEXT NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
            span_id TEXT NOT NULL REFERENCES spans(id) ON DELETE CASCADE,
            sequence_number INTEGER NOT NULL,
            PRIMARY KEY (view_id, turn_id)
        );
        CREATE INDEX IF NOT EXISTS idx_view_selections_span ON view_selections(span_id);
        CREATE INDEX IF NOT EXISTS idx_view_selections_seq ON view_selections(view_id, sequence_number);
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

    async fn create_turn(&self, role: SpanRole) -> Result<Stored<TurnId, Turn>> {
        let conn = self.conn().lock().unwrap();
        let turn_id = TurnId::new();
        let now = unix_timestamp();

        conn.execute(
            "INSERT INTO turns (id, role, created_at) VALUES (?1, ?2, ?3)",
            params![turn_id.as_str(), role.as_str(), now],
        )?;

        Ok(stored(turn_id, Turn::new(role), now))
    }

    async fn get_turn(&self, turn_id: &TurnId) -> Result<Option<Stored<TurnId, Turn>>> {
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
                    .parse::<SpanRole>()
                    .map_err(|_| anyhow::anyhow!("Invalid role: {}", role_str))?;
                Ok(Some(stored(id, Turn::new(role), created)))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    // ========== Span Management ==========

    async fn create_span(&self, turn_id: &TurnId, model_id: Option<&str>) -> Result<Stored<SpanId, Span>> {
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

    async fn get_spans(&self, turn_id: &TurnId) -> Result<Vec<Stored<SpanId, Span>>> {
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

    async fn get_span(&self, span_id: &SpanId) -> Result<Option<Stored<SpanId, Span>>> {
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
        role: MessageRole,
        content: &[StoredContent],
    ) -> Result<Stored<MessageId, Message>> {
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

        let messages: Vec<Stored<MessageId, Message>> = stmt
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
                let role = role_str.parse::<MessageRole>().ok()?;
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

    async fn get_message(&self, message_id: &MessageId) -> Result<Option<Stored<MessageId, Message>>> {
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
                    .parse::<MessageRole>()
                    .map_err(|_| anyhow::anyhow!("Invalid role: {}", role_str))?;
                let message = Message::new(sid, seq, role);
                Ok(Some(stored(id, message, created)))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    // ========== View Management ==========

    async fn create_view(&self) -> Result<Stored<ViewId, View>> {
        let conn = self.conn().lock().unwrap();
        let id = ViewId::new();
        let now = unix_timestamp();

        conn.execute(
            "INSERT INTO views (id, created_at) VALUES (?1, ?2)",
            params![id, now],
        )?;

        Ok(stored(id, View::new(), now))
    }

    async fn get_view(&self, view_id: &ViewId) -> Result<Option<Stored<ViewId, View>>> {
        let conn = self.conn().lock().unwrap();
        let result = conn.query_row(
            "SELECT v.id, v.forked_from_view_id, v.forked_at_turn_id, v.created_at,
                    (SELECT COUNT(*) FROM view_selections vs WHERE vs.view_id = v.id) as turn_count
             FROM views v WHERE v.id = ?1",
            params![view_id],
            |row| {
                let id: ViewId = row.get(0)?;
                let forked_from: Option<ViewId> = row.get(1)?;
                let forked_at: Option<TurnId> = row.get(2)?;
                let created: i64 = row.get(3)?;
                let turn_count: usize = row.get(4)?;
                Ok((id, forked_from, forked_at, created, turn_count))
            },
        );

        match result {
            Ok((id, forked_from, forked_at, created, turn_count)) => {
                let fork = match (forked_from, forked_at) {
                    (Some(from_view_id), Some(at_turn_id)) => Some(ForkInfo { from_view_id, at_turn_id }),
                    _ => None,
                };
                let view = View { fork, turn_count };
                Ok(Some(stored(id, view, created)))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn select_span(
        &self,
        view_id: &ViewId,
        turn_id: &TurnId,
        span_id: &SpanId,
    ) -> Result<()> {
        let conn = self.conn().lock().unwrap();

        // Get next sequence number for this view (only used for new insertions)
        let sequence_number: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(sequence_number), -1) + 1 FROM view_selections WHERE view_id = ?1",
                params![view_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        conn.execute(
            "INSERT INTO view_selections (view_id, turn_id, span_id, sequence_number)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(view_id, turn_id) DO UPDATE SET span_id = ?3",
            params![view_id, turn_id, span_id, sequence_number],
        )?;
        Ok(())
    }

    async fn get_selected_span(
        &self,
        view_id: &ViewId,
        turn_id: &TurnId,
    ) -> Result<Option<SpanId>> {
        let conn = self.conn().lock().unwrap();
        let result = conn.query_row(
            "SELECT span_id FROM view_selections WHERE view_id = ?1 AND turn_id = ?2",
            params![view_id, turn_id],
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

    async fn get_view_path(&self, view_id: &ViewId) -> Result<Vec<TurnWithContent>> {
        // Get all selections for this view, ordered by sequence_number
        let selections: Vec<(TurnId, SpanId)> = {
            let conn = self.conn().lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT turn_id, span_id FROM view_selections
                 WHERE view_id = ?1
                 ORDER BY sequence_number",
            )?;

            let rows = stmt.query_map(params![view_id], |row| {
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

    async fn fork_view(
        &self,
        view_id: &ViewId,
        at_turn_id: &TurnId,
    ) -> Result<Stored<ViewId, View>> {
        let new_id = ViewId::new();
        let now = unix_timestamp();

        let conn = self.conn().lock().unwrap();

        // Get the sequence number of the fork point in the original view
        let fork_seq: i32 = conn.query_row(
            "SELECT sequence_number FROM view_selections WHERE view_id = ?1 AND turn_id = ?2",
            params![view_id, at_turn_id],
            |row| row.get(0),
        )?;

        conn.execute(
            "INSERT INTO views (id, forked_from_view_id, forked_at_turn_id, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![new_id, view_id, at_turn_id, now],
        )?;

        // Copy selections before the fork point (sequence_number < fork_seq)
        conn.execute(
            "INSERT INTO view_selections (view_id, turn_id, span_id, sequence_number)
             SELECT ?1, vs.turn_id, vs.span_id, vs.sequence_number
             FROM view_selections vs
             WHERE vs.view_id = ?2
               AND vs.sequence_number < ?3",
            params![new_id, view_id, fork_seq],
        )?;

        // turn_count is the number of copied selections (fork_seq since sequence starts at 0)
        let turn_count = fork_seq as usize;

        let view = View::forked(view_id.clone(), at_turn_id.clone(), turn_count);
        Ok(stored(new_id, view, now))
    }

    async fn get_view_context_at(
        &self,
        view_id: &ViewId,
        up_to_turn_id: &TurnId,
    ) -> Result<Vec<TurnWithContent>> {
        // Get sequence number of the up_to turn in this view
        let up_to_seq: i32 = {
            let conn = self.conn().lock().unwrap();
            conn.query_row(
                "SELECT sequence_number FROM view_selections WHERE view_id = ?1 AND turn_id = ?2",
                params![view_id, up_to_turn_id],
                |row| row.get(0),
            )?
        };

        // Get all selections before the up_to turn
        let selections: Vec<(TurnId, SpanId)> = {
            let conn = self.conn().lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT turn_id, span_id FROM view_selections
                 WHERE view_id = ?1 AND sequence_number < ?2
                 ORDER BY sequence_number",
            )?;

            let rows = stmt.query_map(params![view_id, up_to_seq], |row| {
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
    use super::*;

    fn create_test_store() -> SqliteStore {
        SqliteStore::in_memory().unwrap()
    }

    #[tokio::test]
    async fn test_turn_crud() {
        let store = create_test_store();

        // Create user turn
        let turn1 = store.create_turn(SpanRole::User).await.unwrap();
        assert_eq!(turn1.role, SpanRole::User);

        // Create assistant turn
        let turn2 = store.create_turn(SpanRole::Assistant).await.unwrap();
        assert_eq!(turn2.role, SpanRole::Assistant);

        // Get turns individually
        let fetched = store.get_turn(&turn1.id).await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().id, turn1.id);
    }

    #[tokio::test]
    async fn test_span_and_message() {
        let store = create_test_store();

        // Create turn
        let turn = store.create_turn(SpanRole::User).await.unwrap();

        // Create span
        let span = store.create_span(&turn.id, None).await.unwrap();
        assert_eq!(span.message_count, 0);

        // Add message
        let content_block_id = ContentBlockId::new();
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
        let store = create_test_store();

        // Create view
        let view = store.create_view().await.unwrap();

        // Create user turn with span and message, select in view
        let turn1 = store.create_turn(SpanRole::User).await.unwrap();
        let span1 = store.create_span(&turn1.id, None).await.unwrap();
        let content_block_id = ContentBlockId::new();
        let content = vec![StoredContent::text_ref(content_block_id)];
        store.add_message(&span1.id, MessageRole::User, &content).await.unwrap();
        store.select_span(&view.id, &turn1.id, &span1.id).await.unwrap();

        // Create assistant turn with span and message, select in view
        let turn2 = store.create_turn(SpanRole::Assistant).await.unwrap();
        let span2 = store.create_span(&turn2.id, Some("claude")).await.unwrap();
        let content_block_id2 = ContentBlockId::new();
        let content2 = vec![StoredContent::text_ref(content_block_id2)];
        store.add_message(&span2.id, MessageRole::Assistant, &content2).await.unwrap();
        store.select_span(&view.id, &turn2.id, &span2.id).await.unwrap();

        // Get view path
        let path = store.get_view_path(&view.id).await.unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[0].turn.role, SpanRole::User);
        assert_eq!(path[1].turn.role, SpanRole::Assistant);
    }
}
