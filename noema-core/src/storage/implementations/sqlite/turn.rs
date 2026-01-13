//! SQLite implementation of TurnStore

use anyhow::Result;
use async_trait::async_trait;
use rusqlite::{params, Connection};

use super::text::store_content_sync;
use super::SqliteStore;
use crate::storage::Asset;
use crate::storage::content::StoredContent;
use crate::storage::helper::unix_timestamp;
use crate::storage::ids::{
    AssetId, ContentBlockId, ConversationId, DocumentId, MessageContentId, MessageId, SpanId, TurnId, ViewId
};
use crate::storage::traits::TurnStore;
use crate::storage::types::{
    MessageContentInfo, MessageInfo, MessageRole, MessageWithContent, SpanInfo, SpanRole,
    TurnInfo, TurnWithContent, ViewInfo,
};

/// Initialize turn-related schema (turns, spans, messages, views)
pub(crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Turns: positions in conversation sequence
        CREATE TABLE IF NOT EXISTS turns (
            id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
            role TEXT CHECK(role IN ('user', 'assistant')) NOT NULL,
            sequence_number INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            UNIQUE (conversation_id, sequence_number)
        );
        CREATE INDEX IF NOT EXISTS idx_turns_conversation ON turns(conversation_id, sequence_number);

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
            filename TEXT,
            -- For document_ref: reference to document
            document_id TEXT,
            -- For tool_call/tool_result: structured JSON
            tool_data TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_message_content_message ON message_content(message_id, sequence_number);
        CREATE INDEX IF NOT EXISTS idx_message_content_block ON message_content(content_block_id);

        -- Views: named paths through conversation
        CREATE TABLE IF NOT EXISTS views (
            id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
            name TEXT,
            is_main INTEGER NOT NULL DEFAULT 0,
            forked_from_view_id TEXT REFERENCES views(id),
            forked_at_turn_id TEXT REFERENCES turns(id),
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_views_conversation ON views(conversation_id);

        -- View selections: which span is selected at each turn for a view
        CREATE TABLE IF NOT EXISTS view_selections (
            view_id TEXT NOT NULL REFERENCES views(id) ON DELETE CASCADE,
            turn_id TEXT NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
            span_id TEXT NOT NULL REFERENCES spans(id) ON DELETE CASCADE,
            PRIMARY KEY (view_id, turn_id)
        );
        CREATE INDEX IF NOT EXISTS idx_view_selections_span ON view_selections(span_id);
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
) -> Result<Vec<MessageContentInfo>> {
    let mut stmt = conn.prepare(
        "SELECT id, message_id, sequence_number, content_type,
                content_block_id, asset_id, mime_type, filename, document_id, tool_data
         FROM message_content WHERE message_id = ?1
         ORDER BY sequence_number",
    )?;

    let content = stmt
        .query_map(params![message_id.as_str()], |row| {
            let id: String = row.get(0)?;
            let mid: String = row.get(1)?;
            let seq: i32 = row.get(2)?;
            let content_type: String = row.get(3)?;
            let content_block_id: Option<ContentBlockId> = row.get(4)?;
            let asset_id: Option<AssetId> = row.get(5)?;
            let mime_type: Option<String> = row.get(6)?;
            let filename: Option<String> = row.get(7)?;
            let document_id: Option<DocumentId> = row.get(8)?;
            let tool_data: Option<String> = row.get(9)?;
            Ok((
                id,
                mid,
                seq,
                content_type,
                content_block_id,
                asset_id,
                mime_type,
                filename,
                document_id,
                tool_data,
            ))
        })?
        .filter_map(|r| r.ok())
        .filter_map(
            |(
                id,
                mid,
                seq,
                content_type,
                content_block_id,
                asset_id,
                mime_type,
                filename,
                document_id,
                tool_data,
            )| {
                let content = match content_type.as_str() {
                    "text" => {
                        StoredContent::TextRef {
                            content_block_id: content_block_id?,
                        }
                    }
                    "asset_ref" => StoredContent::AssetRef {
                        asset_id: asset_id?,
                        mime_type: mime_type?,
                        filename,
                    },
                    "document_ref" => StoredContent::DocumentRef {
                        document_id: document_id?,
                    },
                    "tool_call" => {
                        let data = tool_data?;
                        let call: llm::ToolCall = serde_json::from_str(&data).ok()?;
                        StoredContent::ToolCall(call)
                    }
                    "tool_result" => {
                        let data = tool_data?;
                        let result: llm::ToolResult = serde_json::from_str(&data).ok()?;
                        StoredContent::ToolResult(result)
                    }
                    _ => return None,
                };
                Some(MessageContentInfo {
                    id: MessageContentId::from_string(id),
                    message_id: MessageId::from_string(mid),
                    sequence_number: seq,
                    content,
                })
            },
        )
        .collect();

    Ok(content)
}

// ============================================================================
// TurnStore Implementation
// ============================================================================

#[async_trait]
impl TurnStore for SqliteStore {
    // ========== Turn Management ==========

    async fn add_turn(
        &self,
        conversation_id: &ConversationId,
        role: SpanRole,
    ) -> Result<TurnInfo> {
        let conn = self.conn().lock().unwrap();
        let id = TurnId::new();
        let now = unix_timestamp();

        // Get next sequence number
        let sequence_number: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(sequence_number), -1) + 1 FROM turns WHERE conversation_id = ?1",
                params![conversation_id.as_str()],
                |row| row.get(0),
            )
            .unwrap_or(0);

        conn.execute(
            "INSERT INTO turns (id, conversation_id, role, sequence_number, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id.as_str(),
                conversation_id.as_str(),
                role.as_str(),
                sequence_number,
                now
            ],
        )?;

        Ok(TurnInfo {
            id,
            conversation_id: conversation_id.clone(),
            role,
            sequence_number,
            created_at: now,
        })
    }

    async fn get_turns(&self, conversation_id: &ConversationId) -> Result<Vec<TurnInfo>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, role, sequence_number, created_at
             FROM turns WHERE conversation_id = ?1
             ORDER BY sequence_number",
        )?;

        let turns = stmt
            .query_map(params![conversation_id.as_str()], |row| {
                let id: String = row.get(0)?;
                let conv_id: String = row.get(1)?;
                let role_str: String = row.get(2)?;
                let seq: i32 = row.get(3)?;
                let created: i64 = row.get(4)?;
                Ok((id, conv_id, role_str, seq, created))
            })?
            .filter_map(|r| r.ok())
            .filter_map(|(id, conv_id, role_str, seq, created)| {
                let role = role_str.parse::<SpanRole>().ok()?;
                Some(TurnInfo {
                    id: TurnId::from_string(id),
                    conversation_id: ConversationId::from_string(conv_id),
                    role,
                    sequence_number: seq,
                    created_at: created,
                })
            })
            .collect();

        Ok(turns)
    }

    async fn get_turn(&self, turn_id: &TurnId) -> Result<Option<TurnInfo>> {
        let conn = self.conn().lock().unwrap();
        let result = conn.query_row(
            "SELECT id, conversation_id, role, sequence_number, created_at
             FROM turns WHERE id = ?1",
            params![turn_id.as_str()],
            |row| {
                let id: String = row.get(0)?;
                let conv_id: String = row.get(1)?;
                let role_str: String = row.get(2)?;
                let seq: i32 = row.get(3)?;
                let created: i64 = row.get(4)?;
                Ok((id, conv_id, role_str, seq, created))
            },
        );

        match result {
            Ok((id, conv_id, role_str, seq, created)) => {
                let role = role_str
                    .parse::<SpanRole>()
                    .map_err(|_| anyhow::anyhow!("Invalid role: {}", role_str))?;
                Ok(Some(TurnInfo {
                    id: TurnId::from_string(id),
                    conversation_id: ConversationId::from_string(conv_id),
                    role,
                    sequence_number: seq,
                    created_at: created,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    // ========== Span Management ==========

    async fn add_span(&self, turn_id: &TurnId, model_id: Option<&str>) -> Result<SpanInfo> {
        let conn = self.conn().lock().unwrap();
        let id = SpanId::new();
        let now = unix_timestamp();

        conn.execute(
            "INSERT INTO spans (id, turn_id, model_id, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![id.as_str(), turn_id.as_str(), model_id, now],
        )?;

        Ok(SpanInfo {
            id,
            turn_id: turn_id.clone(),
            model_id: model_id.map(|s| s.to_string()),
            message_count: 0,
            created_at: now,
        })
    }

    async fn get_spans(&self, turn_id: &TurnId) -> Result<Vec<SpanInfo>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT s.id, s.turn_id, s.model_id, s.created_at,
                    (SELECT COUNT(*) FROM messages m WHERE m.span_id = s.id) as message_count
             FROM spans s WHERE s.turn_id = ?1
             ORDER BY s.created_at",
        )?;

        let spans = stmt
            .query_map(params![turn_id.as_str()], |row| {
                let id: SpanId = row.get(0)?;
                let tid: TurnId = row.get(1)?;
                let model: Option<String> = row.get(2)?;
                let created: i64 = row.get(3)?;
                let msg_count: i32 = row.get(4)?;
                Ok((id, tid, model, created, msg_count))
            })?
            .filter_map(|r| r.ok())
            .map(|(id, tid, model, created, msg_count)| SpanInfo {
                id: id,
                turn_id: tid,
                model_id: model,
                message_count: msg_count,
                created_at: created,
            })
            .collect();

        Ok(spans)
    }

    async fn get_span(&self, span_id: &SpanId) -> Result<Option<SpanInfo>> {
        let conn = self.conn().lock().unwrap();
        let result = conn.query_row(
            "SELECT s.id, s.turn_id, s.model_id, s.created_at,
                    (SELECT COUNT(*) FROM messages m WHERE m.span_id = s.id) as message_count
             FROM spans s WHERE s.id = ?1",
            params![span_id.as_str()],
            |row| {
                let id: String = row.get(0)?;
                let tid: String = row.get(1)?;
                let model: Option<String> = row.get(2)?;
                let created: i64 = row.get(3)?;
                let msg_count: i32 = row.get(4)?;
                Ok((id, tid, model, created, msg_count))
            },
        );

        match result {
            Ok((id, tid, model, created, msg_count)) => Ok(Some(SpanInfo {
                id: SpanId::from_string(id),
                turn_id: TurnId::from_string(tid),
                model_id: model,
                message_count: msg_count,
                created_at: created,
            })),
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
    ) -> Result<MessageInfo> {
        let conn = self.conn().lock().unwrap();
        let message_id = MessageId::new();
        let now = unix_timestamp();

        // Get next sequence number for message
        let sequence_number: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(sequence_number), -1) + 1 FROM messages WHERE span_id = ?1",
                params![span_id.as_str()],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Insert message row
        conn.execute(
            "INSERT INTO messages (id, span_id, sequence_number, role, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                message_id.as_str(),
                span_id.as_str(),
                sequence_number,
                role.as_str(),
                now
            ],
        )?;

        // Insert content items
        insert_message_content(&conn, &message_id, content)?;

        Ok(MessageInfo {
            id: message_id,
            span_id: span_id.clone(),
            sequence_number,
            role,
            created_at: now,
        })
    }

    async fn get_messages(&self, span_id: &SpanId) -> Result<Vec<MessageInfo>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, span_id, sequence_number, role, created_at
             FROM messages WHERE span_id = ?1
             ORDER BY sequence_number",
        )?;

        let messages = stmt
            .query_map(params![span_id.as_str()], |row| {
                let id: String = row.get(0)?;
                let sid: String = row.get(1)?;
                let seq: i32 = row.get(2)?;
                let role_str: String = row.get(3)?;
                let created: i64 = row.get(4)?;
                Ok((id, sid, seq, role_str, created))
            })?
            .filter_map(|r| r.ok())
            .filter_map(|(id, sid, seq, role_str, created)| {
                let role = role_str.parse::<MessageRole>().ok()?;
                Some(MessageInfo {
                    id: MessageId::from_string(id),
                    span_id: SpanId::from_string(sid),
                    sequence_number: seq,
                    role,
                    created_at: created,
                })
            })
            .collect();

        Ok(messages)
    }

    async fn get_messages_with_content(
        &self,
        span_id: &SpanId,
    ) -> Result<Vec<MessageWithContent>> {
        let messages = self.get_messages(span_id).await?;
        let conn = self.conn().lock().unwrap();

        let mut result = Vec::new();
        for message in messages {
            let content = load_message_content(&conn, &message.id)?;
            result.push(MessageWithContent { message, content });
        }

        Ok(result)
    }

    async fn get_message(&self, message_id: &MessageId) -> Result<Option<MessageInfo>> {
        let conn = self.conn().lock().unwrap();
        let result = conn.query_row(
            "SELECT id, span_id, sequence_number, role, created_at
             FROM messages WHERE id = ?1",
            params![message_id.as_str()],
            |row| {
                let id: String = row.get(0)?;
                let sid: String = row.get(1)?;
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
                Ok(Some(MessageInfo {
                    id: MessageId::from_string(id),
                    span_id: SpanId::from_string(sid),
                    sequence_number: seq,
                    role,
                    created_at: created,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    // ========== View Management ==========

    async fn create_view(
        &self,
        conversation_id: &ConversationId,
        name: Option<&str>,
        is_main: bool,
    ) -> Result<ViewInfo> {
        let conn = self.conn().lock().unwrap();
        let id = ViewId::new();
        let now = unix_timestamp();

        conn.execute(
            "INSERT INTO views (id, conversation_id, name, is_main, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id.as_str(),
                conversation_id.as_str(),
                name,
                is_main as i32,
                now
            ],
        )?;

        Ok(ViewInfo {
            id,
            conversation_id: conversation_id.clone(),
            name: name.map(|s| s.to_string()),
            is_main,
            forked_from_view_id: None,
            forked_at_turn_id: None,
            created_at: now,
        })
    }

    async fn get_views(&self, conversation_id: &ConversationId) -> Result<Vec<ViewInfo>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, name, is_main, forked_from_view_id, forked_at_turn_id, created_at
             FROM views WHERE conversation_id = ?1
             ORDER BY created_at",
        )?;

        let views = stmt
            .query_map(params![conversation_id.as_str()], |row| {
                let id: String = row.get(0)?;
                let cid: String = row.get(1)?;
                let name: Option<String> = row.get(2)?;
                let is_main_int: i32 = row.get(3)?;
                let forked_from: Option<String> = row.get(4)?;
                let forked_at: Option<String> = row.get(5)?;
                let created: i64 = row.get(6)?;
                Ok((id, cid, name, is_main_int, forked_from, forked_at, created))
            })?
            .filter_map(|r| r.ok())
            .map(
                |(id, cid, name, is_main_int, forked_from, forked_at, created)| ViewInfo {
                    id: ViewId::from_string(id),
                    conversation_id: ConversationId::from_string(cid),
                    name,
                    is_main: is_main_int != 0,
                    forked_from_view_id: forked_from.map(ViewId::from_string),
                    forked_at_turn_id: forked_at.map(TurnId::from_string),
                    created_at: created,
                },
            )
            .collect();

        Ok(views)
    }

    async fn get_main_view(&self, conversation_id: &ConversationId) -> Result<Option<ViewInfo>> {
        let conn = self.conn().lock().unwrap();
        let result = conn.query_row(
            "SELECT id, conversation_id, name, is_main, forked_from_view_id, forked_at_turn_id, created_at
             FROM views WHERE conversation_id = ?1 AND is_main = 1",
            params![conversation_id.as_str()],
            |row| {
                let id: String = row.get(0)?;
                let cid: String = row.get(1)?;
                let name: Option<String> = row.get(2)?;
                let is_main_int: i32 = row.get(3)?;
                let forked_from: Option<String> = row.get(4)?;
                let forked_at: Option<String> = row.get(5)?;
                let created: i64 = row.get(6)?;
                Ok((id, cid, name, is_main_int, forked_from, forked_at, created))
            },
        );

        match result {
            Ok((id, cid, name, is_main_int, forked_from, forked_at, created)) => Ok(Some(ViewInfo {
                id: ViewId::from_string(id),
                conversation_id: ConversationId::from_string(cid),
                name,
                is_main: is_main_int != 0,
                forked_from_view_id: forked_from.map(ViewId::from_string),
                forked_at_turn_id: forked_at.map(TurnId::from_string),
                created_at: created,
            })),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn get_view(&self, view_id: &ViewId) -> Result<Option<ViewInfo>> {
        let conn = self.conn().lock().unwrap();
        let result = conn.query_row(
            "SELECT id, conversation_id, name, is_main, forked_from_view_id, forked_at_turn_id, created_at
             FROM views WHERE id = ?1",
            params![view_id.as_str()],
            |row| {
                let id: ViewId = row.get(0)?;
                let cid: ConversationId = row.get(1)?;
                let name: Option<String> = row.get(2)?;
                let is_main_int: i32 = row.get(3)?;
                let forked_from: Option<ViewId> = row.get(4)?;
                let forked_at: Option<TurnId> = row.get(5)?;
                let created: i64 = row.get(6)?;
                Ok((id, cid, name, is_main_int, forked_from, forked_at, created))
            },
        );

        match result {
            Ok((id, cid, name, is_main_int, forked_from, forked_at, created)) => Ok(Some(ViewInfo {
                id,
                conversation_id: cid,
                name,
                is_main: is_main_int != 0,
                forked_from_view_id: forked_from,
                forked_at_turn_id: forked_at,
                created_at: created,
            })),
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
        conn.execute(
            "INSERT INTO view_selections (view_id, turn_id, span_id)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(view_id, turn_id) DO UPDATE SET span_id = ?3",
            params![view_id.as_str(), turn_id.as_str(), span_id.as_str()],
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
            params![view_id.as_str(), turn_id.as_str()],
            |row| {
                let span_id: String = row.get(0)?;
                Ok(span_id)
            },
        );

        match result {
            Ok(span_id) => Ok(Some(SpanId::from_string(span_id))),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn get_view_path(&self, view_id: &ViewId) -> Result<Vec<TurnWithContent>> {
        let conversation_id = {
            let conn = self.conn().lock().unwrap();
            conn.query_row(
                "SELECT conversation_id FROM views WHERE id = ?1",
                params![view_id.as_str()],
                |row| row.get::<_, String>(0),
            )?
        };

        let turns = self
            .get_turns(&ConversationId::from_string(conversation_id))
            .await?;

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
        let conn = self.conn().lock().unwrap();
        let (conversation_id, _): (ConversationId, i32) = conn.query_row(
            "SELECT conversation_id, is_main FROM views WHERE id = ?1",
            params![view_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        drop(conn);

        let new_id = ViewId::new();
        let now = unix_timestamp();

        let conn = self.conn().lock().unwrap();
        conn.execute(
            "INSERT INTO views (id, conversation_id, name, is_main, forked_from_view_id, forked_at_turn_id, created_at)
             VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6)",
            params![
                new_id.as_str(),
                &conversation_id,
                name,
                view_id.as_str(),
                at_turn_id.as_str(),
                now
            ],
        )?;

        conn.execute(
            "INSERT INTO view_selections (view_id, turn_id, span_id)
             SELECT ?1, vs.turn_id, vs.span_id
             FROM view_selections vs
             JOIN turns t ON t.id = vs.turn_id
             JOIN turns fork_turn ON fork_turn.id = ?3
             WHERE vs.view_id = ?2
               AND t.sequence_number < fork_turn.sequence_number",
            params![new_id.as_str(), view_id.as_str(), at_turn_id.as_str()],
        )?;

        Ok(ViewInfo {
            id: new_id,
            conversation_id: conversation_id,
            name: name.map(|s| s.to_string()),
            is_main: false,
            forked_from_view_id: Some(view_id.clone()),
            forked_at_turn_id: Some(at_turn_id.clone()),
            created_at: now,
        })
    }

    async fn fork_view_with_selections(
        &self,
        view_id: &ViewId,
        at_turn_id: &TurnId,
        name: Option<&str>,
        selections: &[(TurnId, SpanId)],
    ) -> Result<ViewInfo> {
        let conn = self.conn().lock().unwrap();
        let conversation_id: String = conn.query_row(
            "SELECT conversation_id FROM views WHERE id = ?1",
            params![view_id.as_str()],
            |row| row.get(0),
        )?;
        drop(conn);

        let new_id = ViewId::new();
        let now = unix_timestamp();

        let conn = self.conn().lock().unwrap();
        conn.execute(
            "INSERT INTO views (id, conversation_id, name, is_main, forked_from_view_id, forked_at_turn_id, created_at)
             VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6)",
            params![
                new_id.as_str(),
                &conversation_id,
                name,
                view_id.as_str(),
                at_turn_id.as_str(),
                now
            ],
        )?;

        conn.execute(
            "INSERT INTO view_selections (view_id, turn_id, span_id)
             SELECT ?1, vs.turn_id, vs.span_id
             FROM view_selections vs
             JOIN turns t ON t.id = vs.turn_id
             JOIN turns fork_turn ON fork_turn.id = ?3
             WHERE vs.view_id = ?2
               AND t.sequence_number < fork_turn.sequence_number",
            params![new_id.as_str(), view_id.as_str(), at_turn_id.as_str()],
        )?;

        for (turn_id, span_id) in selections {
            conn.execute(
                "INSERT INTO view_selections (view_id, turn_id, span_id)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(view_id, turn_id) DO UPDATE SET span_id = ?3",
                params![new_id.as_str(), turn_id.as_str(), span_id.as_str()],
            )?;
        }

        Ok(ViewInfo {
            id: new_id,
            conversation_id: ConversationId::from_string(conversation_id),
            name: name.map(|s| s.to_string()),
            is_main: false,
            forked_from_view_id: Some(view_id.clone()),
            forked_at_turn_id: Some(at_turn_id.clone()),
            created_at: now,
        })
    }

    async fn get_view_context_at(
        &self,
        view_id: &ViewId,
        up_to_turn_id: &TurnId,
    ) -> Result<Vec<TurnWithContent>> {
        let (conversation_id, up_to_seq) = {
            let conn = self.conn().lock().unwrap();
            let conv_id: String = conn.query_row(
                "SELECT conversation_id FROM views WHERE id = ?1",
                params![view_id.as_str()],
                |row| row.get(0),
            )?;
            let seq: i32 = conn.query_row(
                "SELECT sequence_number FROM turns WHERE id = ?1",
                params![up_to_turn_id.as_str()],
                |row| row.get(0),
            )?;
            (conv_id, seq)
        };

        let turns: Vec<TurnInfo> = {
            let conn = self.conn().lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT id, conversation_id, role, sequence_number, created_at
                 FROM turns
                 WHERE conversation_id = ?1 AND sequence_number < ?2
                 ORDER BY sequence_number",
            )?;

            let rows = stmt.query_map(params![&conversation_id, up_to_seq], |row| {
                let id: String = row.get(0)?;
                let conv_id: String = row.get(1)?;
                let role_str: String = row.get(2)?;
                let seq: i32 = row.get(3)?;
                let created: i64 = row.get(4)?;
                Ok((id, conv_id, role_str, seq, created))
            })?;

            let mut turns = Vec::new();
            for row in rows {
                if let Ok((id, conv_id, role_str, seq, created)) = row {
                    if let Ok(role) = role_str.parse::<SpanRole>() {
                        turns.push(TurnInfo {
                            id: TurnId::from_string(id),
                            conversation_id: ConversationId::from_string(conv_id),
                            role,
                            sequence_number: seq,
                            created_at: created,
                        });
                    }
                }
            }
            turns
        };

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
        let content_block_id = {
            let conn = self.conn().lock().unwrap();
            let id = store_content_sync(&conn, text, Some("user"), None, None)?;
            ContentBlockId::from_string(id)
        };

        let turn = self.add_turn(conversation_id, SpanRole::User).await?;
        let span = self.add_span(&turn.id, None).await?;
        let content = vec![StoredContent::text_ref(content_block_id)];
        let message = self
            .add_message(&span.id, MessageRole::User, &content)
            .await?;

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
        let content_block_id = {
            let conn = self.conn().lock().unwrap();
            let id = store_content_sync(&conn, text, Some("assistant"), Some(model_id), None)?;
            ContentBlockId::from_string(id)
        };

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
                        content_id.as_str(),
                        message_id.as_str(),
                        content_seq as i32,
                        "text",
                        content_block_id.as_str()
                    ],
                )?;
            }
            StoredContent::AssetRef {
                asset_id,
                mime_type,
                filename,
            } => {
                conn.execute(
                    "INSERT INTO message_content (id, message_id, sequence_number, content_type, asset_id, mime_type, filename)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        content_id.as_str(),
                        message_id.as_str(),
                        content_seq as i32,
                        "asset_ref",
                        asset_id.as_str(),
                        mime_type,
                        filename.as_deref()
                    ],
                )?;
            }
            StoredContent::DocumentRef { document_id } => {
                conn.execute(
                    "INSERT INTO message_content (id, message_id, sequence_number, content_type, document_id)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        content_id.as_str(),
                        message_id.as_str(),
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
                        content_id.as_str(),
                        message_id.as_str(),
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
                        content_id.as_str(),
                        message_id.as_str(),
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

// ============================================================================
// Synchronous Helpers
// ============================================================================

/// Helper functions for writing to TurnStore tables synchronously.
pub mod sync_helpers {
    use anyhow::Result;
    use rusqlite::{params, Connection};

    use crate::storage::helper::unix_timestamp;
    use crate::storage::ids::{ConversationId, MessageId, SpanId, TurnId, ViewId};
    use crate::storage::types::{MessageRole, SpanRole};

    use super::insert_message_content;

    /// Ensure a main view exists for the conversation, creating one if needed.
    pub fn ensure_main_view(conn: &Connection, conversation_id: &ConversationId) -> Result<ViewId> {
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM views WHERE conversation_id = ?1 AND is_main = 1",
                params![conversation_id.as_str()],
                |row| row.get(0),
            )
            .ok();

        if let Some(id) = existing {
            return Ok(ViewId::from_string(id));
        }

        let id = ViewId::new();
        let now = unix_timestamp();
        conn.execute(
            "INSERT INTO views (id, conversation_id, name, is_main, created_at)
             VALUES (?1, ?2, 'main', 1, ?3)",
            params![id.as_str(), conversation_id.as_str(), now],
        )?;

        Ok(id)
    }

    /// Add a turn synchronously.
    pub fn add_turn_sync(
        conn: &Connection,
        conversation_id: &ConversationId,
        role: SpanRole,
    ) -> Result<(TurnId, i32)> {
        let id = TurnId::new();
        let now = unix_timestamp();

        let sequence_number: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(sequence_number), -1) + 1 FROM turns WHERE conversation_id = ?1",
                params![conversation_id.as_str()],
                |row| row.get(0),
            )
            .unwrap_or(0);

        conn.execute(
            "INSERT INTO turns (id, conversation_id, role, sequence_number, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id.as_str(),
                conversation_id.as_str(),
                role.as_str(),
                sequence_number,
                now
            ],
        )?;

        Ok((id, sequence_number))
    }

    /// Add a span synchronously.
    pub fn add_span_sync(
        conn: &Connection,
        turn_id: &TurnId,
        model_id: Option<&str>,
    ) -> Result<SpanId> {
        let id = SpanId::new();
        let now = unix_timestamp();

        conn.execute(
            "INSERT INTO spans (id, turn_id, model_id, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![id.as_str(), turn_id.as_str(), model_id, now],
        )?;

        Ok(id)
    }

    /// Add a message synchronously.
    pub fn add_message_sync(
        conn: &Connection,
        span_id: &SpanId,
        role: MessageRole,
        content: &[crate::storage::content::StoredContent],
    ) -> Result<MessageId> {
        let message_id = MessageId::new();
        let now = unix_timestamp();

        let sequence_number: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(sequence_number), -1) + 1 FROM messages WHERE span_id = ?1",
                params![span_id.as_str()],
                |row| row.get(0),
            )
            .unwrap_or(0);

        conn.execute(
            "INSERT INTO messages (id, span_id, sequence_number, role, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                message_id.as_str(),
                span_id.as_str(),
                sequence_number,
                role.as_str(),
                now
            ],
        )?;

        insert_message_content(conn, &message_id, content)?;

        Ok(message_id)
    }

    /// Select a span for a turn in a view.
    pub fn select_span_sync(
        conn: &Connection,
        view_id: &ViewId,
        turn_id: &TurnId,
        span_id: &SpanId,
    ) -> Result<()> {
        conn.execute(
            "INSERT INTO view_selections (view_id, turn_id, span_id)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(view_id, turn_id) DO UPDATE SET span_id = ?3",
            params![view_id.as_str(), turn_id.as_str(), span_id.as_str()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::implementations::sqlite::store_content_sync;

    fn create_test_store() -> SqliteStore {
        SqliteStore::in_memory().unwrap()
    }

    fn create_test_conversation(store: &SqliteStore) -> ConversationId {
        let id = ConversationId::new();
        let now = unix_timestamp();
        let conn = store.conn().lock().unwrap();
        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at)
             VALUES (?1, NULL, 'Test', ?2, ?2)",
            params![id.as_str(), now],
        )
        .unwrap();
        id
    }

    #[tokio::test]
    async fn test_turn_crud() {
        let store = create_test_store();
        let conv_id = create_test_conversation(&store);

        // Create view first
        let _view = store
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
        let store = create_test_store();
        let conv_id = create_test_conversation(&store);

        // Create view and turn
        let _view = store
            .create_view(&conv_id, Some("main"), true)
            .await
            .unwrap();
        let turn = store.add_turn(&conv_id, SpanRole::User).await.unwrap();

        // Add span
        let span = store.add_span(&turn.id, None).await.unwrap();
        assert_eq!(span.message_count, 0);

        // Store text and add message
        let text = "Hello, world!";
        let content_block_id = {
            let conn = store.conn().lock().unwrap();
            ContentBlockId::from_string(
                store_content_sync(&conn, text, Some("user"), None, None).unwrap(),
            )
        };
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
        let store = create_test_store();
        let conv_id = create_test_conversation(&store);

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
