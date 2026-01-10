//! SQLite implementation of ConversationStore and TurnStore

use anyhow::{Context, Result};
use async_trait::async_trait;
use llm::api::Role;
use rusqlite::{params, Connection};
use uuid::Uuid;

use super::{
    ConversationInfo, ConversationStore, SpanInfo as LegacySpanInfo, SpanSetInfo, SpanSetWithContent, SpanType,
    ThreadInfo,
};
use super::types::{
    MessageInfo, MessageRole, NewMessage, SpanInfo, SpanRole, TurnInfo,
    TurnStore, TurnWithContent, ViewInfo,
};
use crate::storage::content::{StoredMessage, StoredPayload};
use crate::storage::content_block::sqlite::store_content_sync;
use crate::storage::ids::{
    ContentBlockId, ConversationId, MessageId, SpanId, TurnId, ViewId,
};
use crate::storage::session::SqliteStore;
use crate::storage::helper::unix_timestamp;

pub (crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Conversations
        CREATE TABLE IF NOT EXISTS conversations (
            id TEXT PRIMARY KEY,
            user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
            title TEXT,
            system_prompt TEXT,
            summary_text TEXT,
            summary_embedding BLOB,
            is_private INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        -- Threads: a path through the conversation
        -- parent_span_id points to the specific span this thread forks from (NULL for main thread)
        CREATE TABLE IF NOT EXISTS threads (
            id TEXT PRIMARY KEY,
            conversation_id TEXT REFERENCES conversations(id) ON DELETE CASCADE,
            parent_span_id TEXT REFERENCES spans(id),
            status TEXT NOT NULL DEFAULT 'active',
            created_at INTEGER NOT NULL
        );

        -- Indexes for conversations
        CREATE INDEX IF NOT EXISTS idx_conversations_user ON conversations(user_id);
        CREATE INDEX IF NOT EXISTS idx_threads_conversation ON threads(conversation_id);

        -- SpanSets: positions in conversation (for parallel model responses)
        CREATE TABLE IF NOT EXISTS span_sets (
            id TEXT PRIMARY KEY,
            thread_id TEXT REFERENCES threads(id) ON DELETE CASCADE,
            sequence_number INTEGER NOT NULL,
            span_type TEXT CHECK(span_type IN ('user', 'assistant')) NOT NULL,
            selected_span_id TEXT,
            created_at INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_span_sets_thread ON span_sets(thread_id, sequence_number);

        -- Legacy spans: alternative responses within a SpanSet
        -- Renamed from 'spans' to avoid conflict with new Turn/Span/Message structure
        CREATE TABLE IF NOT EXISTS legacy_spans (
            id TEXT PRIMARY KEY,
            span_set_id TEXT REFERENCES span_sets(id) ON DELETE CASCADE,
            model_id TEXT,
            created_at INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_legacy_spans_span_set ON legacy_spans(span_set_id);

        -- Legacy span messages: individual messages within a span (for multi-turn agentic responses)
        -- Renamed from 'span_messages' to avoid conflict with new messages table
        CREATE TABLE IF NOT EXISTS legacy_span_messages (
            id TEXT PRIMARY KEY,
            span_id TEXT REFERENCES legacy_spans(id) ON DELETE CASCADE,
            sequence_number INTEGER NOT NULL,
            role TEXT CHECK(role IN ('user', 'assistant', 'system', 'tool')) NOT NULL,
            content TEXT NOT NULL,
            text_content TEXT,
            content_id TEXT REFERENCES content_blocks(id),
            created_at INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_legacy_span_messages_span ON legacy_span_messages(span_id, sequence_number);
        CREATE INDEX IF NOT EXISTS idx_legacy_span_messages_content ON legacy_span_messages(content_id);

        -- ============================================================================
        -- Turn/Span/Message structure (Phase 3)
        -- These coexist with the legacy tables during migration
        -- ============================================================================

        -- Turns: positions in conversation sequence (replaces span_sets)
        CREATE TABLE IF NOT EXISTS turns (
            id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
            role TEXT CHECK(role IN ('user', 'assistant')) NOT NULL,
            sequence_number INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            UNIQUE (conversation_id, sequence_number)
        );
        CREATE INDEX IF NOT EXISTS idx_turns_conversation ON turns(conversation_id, sequence_number);

        -- Spans: alternative responses at a turn (replaces legacy_spans)
        CREATE TABLE IF NOT EXISTS spans (
            id TEXT PRIMARY KEY,
            turn_id TEXT NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
            model_id TEXT,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_spans_turn ON spans(turn_id);

        -- Messages: individual messages within a span (replaces span_messages)
        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            span_id TEXT NOT NULL REFERENCES spans(id) ON DELETE CASCADE,
            sequence_number INTEGER NOT NULL,
            role TEXT CHECK(role IN ('user', 'assistant', 'system', 'tool')) NOT NULL,
            content_id TEXT REFERENCES content_blocks(id),
            tool_calls TEXT,
            tool_results TEXT,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_messages_span ON messages(span_id, sequence_number);
        CREATE INDEX IF NOT EXISTS idx_messages_content ON messages(content_id);

        -- Views: named paths through conversation (replaces threads)
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
    )
    .context("Failed to initialize conversation schema")?;
    Ok(())
}

#[async_trait]
impl ConversationStore for SqliteStore {
    // ========== Conversation Methods ==========

    async fn list_conversations(&self, user_id: &str) -> Result<Vec<ConversationInfo>> {
        let conn = self.conn().lock().unwrap();

        // Count span_sets as the "message count" (each span_set is a turn in conversation)
        let query = "SELECT c.id, c.title, COUNT(ss.id) as msg_count, c.is_private, c.created_at, c.updated_at
             FROM conversations c
             LEFT JOIN threads t ON t.conversation_id = c.id AND t.parent_span_id IS NULL
             LEFT JOIN span_sets ss ON ss.thread_id = t.id
             WHERE c.user_id = ?1
             GROUP BY c.id
             ORDER BY c.updated_at DESC";

        let mut stmt = conn.prepare(query)?;
        let infos: Vec<ConversationInfo> = stmt
            .query_map(params![user_id], |row| {
                let is_private_int: i32 = row.get(3)?;
                Ok(ConversationInfo {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    message_count: row.get(2)?,
                    is_private: is_private_int != 0,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(infos)
    }

    async fn rename_conversation(&self, id: &str, name: Option<&str>) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();
        conn.execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, now, id],
        )?;
        Ok(())
    }

    async fn get_conversation_private(&self, id: &str) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        let result: std::result::Result<i32, _> = conn.query_row(
            "SELECT COALESCE(is_private, 0) FROM conversations WHERE id = ?1",
            params![id],
            |row| row.get(0),
        );
        // Return false if conversation doesn't exist yet
        match result {
            Ok(is_private) => Ok(is_private != 0),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    async fn set_conversation_private(&self, id: &str, is_private: bool) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();
        conn.execute(
            "UPDATE conversations SET is_private = ?1, updated_at = ?2 WHERE id = ?3",
            params![is_private as i32, now, id],
        )?;
        Ok(())
    }

    async fn delete_conversation(&self, id: &str) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        // CASCADE should handle all deletions
        conn.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
        Ok(())
    }

    async fn load_stored_messages(&self, conversation_id: &str) -> Result<Vec<StoredMessage>> {
        let conn = self.conn().lock().unwrap();

        // Load from span_messages via the main thread's selected spans
        let query = "SELECT sm.role, sm.content
             FROM legacy_span_messages sm
             JOIN legacy_spans s ON sm.span_id = s.id
             JOIN span_sets ss ON s.span_set_id = ss.id
             JOIN threads t ON ss.thread_id = t.id
             WHERE t.conversation_id = ?1 AND t.parent_span_id IS NULL
               AND s.id = ss.selected_span_id
             ORDER BY ss.sequence_number, sm.sequence_number";

        let mut stmt = conn.prepare(query)?;

        let messages: Vec<StoredMessage> = stmt
            .query_map(params![conversation_id], |row| {
                let role_str: String = row.get(0)?;
                let payload_json: String = row.get(1)?;
                Ok((role_str, payload_json))
            })?
            .filter_map(|r| r.ok())
            .filter_map(|(role_str, payload_json)| {
                let role = role_str.parse::<Role>().ok()?;
                let payload: StoredPayload = serde_json::from_str(&payload_json).ok()?;
                Some(StoredMessage { role, payload })
            })
            .collect();

        Ok(messages)
    }

    // ========== Thread Methods ==========

    async fn get_main_thread_id(&self, conversation_id: &str) -> Result<Option<String>> {
        let conn = self.conn().lock().unwrap();
        match conn.query_row(
            "SELECT id FROM threads WHERE conversation_id = ?1 AND parent_span_id IS NULL",
            params![conversation_id],
            |row| row.get(0),
        ) {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn create_fork_thread(
        &self,
        conversation_id: &str,
        parent_span_id: &str,
        name: Option<&str>,
    ) -> Result<String> {
        let conn = self.conn().lock().unwrap();
        let id = Uuid::new_v4().to_string();
        let now = unix_timestamp();

        conn.execute(
            "INSERT INTO threads (id, conversation_id, parent_span_id, status, created_at)
             VALUES (?1, ?2, ?3, 'active', ?4)",
            params![&id, conversation_id, parent_span_id, now],
        )?;

        if let Some(n) = name {
            conn.execute(
                "UPDATE threads SET status = ?1 WHERE id = ?2",
                params![format!("active:{}", n), &id],
            )?;
        }

        Ok(id)
    }

    async fn create_fork_conversation(
        &self,
        user_id: &str,
        parent_span_id: &str,
        name: Option<&str>,
    ) -> Result<(String, String)> {
        let conn = self.conn().lock().unwrap();
        let conversation_id = Uuid::new_v4().to_string();
        let thread_id = Uuid::new_v4().to_string();
        let now = unix_timestamp();

        // Create the new conversation
        let title = name.unwrap_or("Fork");
        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![&conversation_id, user_id, title, now, now],
        )?;

        // Create the main thread for this conversation, with parent_span_id pointing to fork point
        conn.execute(
            "INSERT INTO threads (id, conversation_id, parent_span_id, status, created_at)
             VALUES (?1, ?2, ?3, 'active', ?4)",
            params![&thread_id, &conversation_id, parent_span_id, now],
        )?;

        Ok((conversation_id, thread_id))
    }

    async fn list_conversation_threads(&self, conversation_id: &str) -> Result<Vec<ThreadInfo>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, conversation_id, parent_span_id, status, created_at
             FROM threads WHERE conversation_id = ?1 ORDER BY created_at",
        )?;

        let threads = stmt
            .query_map(params![conversation_id], |row| {
                let status: String = row.get(3)?;
                // Parse name from status if present (format: "active:name" or just "active")
                let (status_str, name) = if let Some(idx) = status.find(':') {
                    let (s, n) = status.split_at(idx);
                    (s.to_string(), Some(n[1..].to_string()))
                } else {
                    (status.clone(), None)
                };
                Ok(ThreadInfo {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    parent_span_id: row.get(2)?,
                    name,
                    status: status_str,
                    created_at: row.get(4)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(threads)
    }

    async fn get_thread(&self, thread_id: &str) -> Result<Option<ThreadInfo>> {
        let conn = self.conn().lock().unwrap();
        let thread = conn
            .query_row(
                "SELECT id, conversation_id, parent_span_id, status, created_at
                 FROM threads WHERE id = ?1",
                params![thread_id],
                |row| {
                    let status: String = row.get(3)?;
                    let (status_str, name) = if let Some(idx) = status.find(':') {
                        let (s, n) = status.split_at(idx);
                        (s.to_string(), Some(n[1..].to_string()))
                    } else {
                        (status.clone(), None)
                    };
                    Ok(ThreadInfo {
                        id: row.get(0)?,
                        conversation_id: row.get(1)?,
                        parent_span_id: row.get(2)?,
                        name,
                        status: status_str,
                        created_at: row.get(4)?,
                    })
                },
            )
            .ok();
        Ok(thread)
    }

    async fn rename_thread(&self, thread_id: &str, name: Option<&str>) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        let status = match name {
            Some(n) => format!("active:{}", n),
            None => "active".to_string(),
        };
        conn.execute(
            "UPDATE threads SET status = ?1 WHERE id = ?2",
            params![&status, thread_id],
        )?;
        Ok(())
    }

    async fn delete_thread(&self, thread_id: &str) -> Result<bool> {
        let conn = self.conn().lock().unwrap();

        // Check if this is the main thread (parent_span_id IS NULL)
        let is_main: bool = conn
            .query_row(
                "SELECT parent_span_id IS NULL FROM threads WHERE id = ?1",
                params![thread_id],
                |row| row.get(0),
            )
            .unwrap_or(true);

        if is_main {
            return Err(anyhow::anyhow!("Cannot delete the main thread"));
        }

        // Delete the thread (span_sets will cascade)
        let rows = conn.execute("DELETE FROM threads WHERE id = ?1", params![thread_id])?;
        Ok(rows > 0)
    }

    async fn get_thread_parent_span(&self, thread_id: &str) -> Result<Option<String>> {
        let conn = self.conn().lock().unwrap();
        let parent_span_id: Option<String> = conn
            .query_row(
                "SELECT parent_span_id FROM threads WHERE id = ?1",
                params![thread_id],
                |row| row.get(0),
            )
            .ok()
            .flatten();
        Ok(parent_span_id)
    }

    async fn get_span_parent_span_set(&self, span_id: &str) -> Result<Option<String>> {
        let conn = self.conn().lock().unwrap();
        let span_set_id: Option<String> = conn
            .query_row(
                "SELECT span_set_id FROM legacy_spans WHERE id = ?1",
                params![span_id],
                |row| row.get(0),
            )
            .ok();
        Ok(span_set_id)
    }

    async fn get_span_set_thread(&self, span_set_id: &str) -> Result<Option<String>> {
        let conn = self.conn().lock().unwrap();
        let thread_id: Option<String> = conn
            .query_row(
                "SELECT thread_id FROM span_sets WHERE id = ?1",
                params![span_set_id],
                |row| row.get(0),
            )
            .ok();
        Ok(thread_id)
    }

    // ========== SpanSet Methods ==========

    async fn create_span_set(&self, thread_id: &str, span_type: SpanType) -> Result<String> {
        let conn = self.conn().lock().unwrap();
        let id = Uuid::new_v4().to_string();
        let now = unix_timestamp();

        // Get next sequence number for this thread
        let sequence_number: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sequence_number), 0) + 1 FROM span_sets WHERE thread_id = ?1",
                params![thread_id],
                |row| row.get(0),
            )
            .unwrap_or(1);

        conn.execute(
            "INSERT INTO span_sets (id, thread_id, sequence_number, span_type, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![&id, thread_id, sequence_number, span_type.to_string(), now],
        )?;

        Ok(id)
    }

    async fn get_thread_span_sets(&self, thread_id: &str) -> Result<Vec<SpanSetInfo>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, thread_id, sequence_number, span_type, selected_span_id, created_at
             FROM span_sets WHERE thread_id = ?1 ORDER BY sequence_number",
        )?;

        let span_sets = stmt
            .query_map(params![thread_id], |row| {
                let span_type_str: String = row.get(3)?;
                Ok(SpanSetInfo {
                    id: row.get(0)?,
                    thread_id: row.get(1)?,
                    sequence_number: row.get(2)?,
                    span_type: span_type_str.parse::<SpanType>().unwrap_or(SpanType::User),
                    selected_span_id: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(span_sets)
    }

    async fn get_span_set_with_content(
        &self,
        span_set_id: &str,
    ) -> Result<Option<SpanSetWithContent>> {
        let (span_type, selected_span_id) = {
            let conn = self.conn().lock().unwrap();

            // Get span_set info
            let span_set_info: Option<(String, Option<String>)> = conn
                .query_row(
                    "SELECT span_type, selected_span_id FROM span_sets WHERE id = ?1",
                    params![span_set_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .ok();

            let (span_type_str, selected_span_id) = match span_set_info {
                Some(info) => info,
                None => return Ok(None),
            };

            let span_type = span_type_str.parse::<SpanType>().unwrap_or(SpanType::User);
            (span_type, selected_span_id)
        };

        // Get alternates
        let alternates = self.get_span_set_alternates(span_set_id).await?;

        // Get messages from selected span
        let messages = if let Some(ref span_id) = selected_span_id {
            self.get_span_messages(span_id).await?
        } else {
            Vec::new()
        };

        Ok(Some(SpanSetWithContent {
            id: span_set_id.to_string(),
            span_type,
            messages,
            alternates,
        }))
    }

    async fn get_span_set_alternates(&self, span_set_id: &str) -> Result<Vec<LegacySpanInfo>> {
        let conn = self.conn().lock().unwrap();

        // Get selected_span_id for this span_set
        let selected_span_id: Option<String> = conn
            .query_row(
                "SELECT selected_span_id FROM span_sets WHERE id = ?1",
                params![span_set_id],
                |row| row.get(0),
            )
            .ok()
            .flatten();

        let mut stmt = conn.prepare(
            "SELECT s.id, s.model_id, s.created_at,
                    (SELECT COUNT(*) FROM legacy_span_messages WHERE span_id = s.id) as msg_count
             FROM legacy_spans s
             WHERE s.span_set_id = ?1
             ORDER BY s.created_at",
        )?;

        let spans = stmt
            .query_map(params![span_set_id], |row| {
                let id: String = row.get(0)?;
                Ok(LegacySpanInfo {
                    id: id.clone(),
                    model_id: row.get(1)?,
                    created_at: row.get(2)?,
                    message_count: row.get(3)?,
                    is_selected: selected_span_id.as_ref() == Some(&id),
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(spans)
    }

    async fn set_selected_span(&self, span_set_id: &str, span_id: &str) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        conn.execute(
            "UPDATE span_sets SET selected_span_id = ?1 WHERE id = ?2",
            params![span_id, span_set_id],
        )?;
        Ok(())
    }

    // ========== Span Methods ==========

    async fn create_span(&self, span_set_id: &str, model_id: Option<&str>) -> Result<String> {
        let conn = self.conn().lock().unwrap();
        let id = Uuid::new_v4().to_string();
        let now = unix_timestamp();

        conn.execute(
            "INSERT INTO legacy_spans (id, span_set_id, model_id, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![&id, span_set_id, model_id, now],
        )?;

        // If this is the first span in the set, make it the selected one
        conn.execute(
            "UPDATE span_sets SET selected_span_id = ?1
             WHERE id = ?2 AND selected_span_id IS NULL",
            params![&id, span_set_id],
        )?;

        Ok(id)
    }

    async fn add_span_message(
        &self,
        span_id: &str,
        role: Role,
        content: &StoredPayload,
    ) -> Result<String> {
        let conn = self.conn().lock().unwrap();
        let id = Uuid::new_v4().to_string();
        let now = unix_timestamp();

        // Get next sequence number for this span
        let sequence_number: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sequence_number), 0) + 1 FROM legacy_span_messages WHERE span_id = ?1",
                params![span_id],
                |row| row.get(0),
            )
            .unwrap_or(1);

        let content_json = serde_json::to_string(content)?;

        conn.execute(
            "INSERT INTO legacy_span_messages (id, span_id, sequence_number, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![&id, span_id, sequence_number, role.to_string(), &content_json, now],
        )?;

        Ok(id)
    }

    async fn get_span_messages(&self, span_id: &str) -> Result<Vec<StoredMessage>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT role, content FROM legacy_span_messages WHERE span_id = ?1 ORDER BY sequence_number",
        )?;

        let messages = stmt
            .query_map(params![span_id], |row| {
                let role_str: String = row.get(0)?;
                let content_json: String = row.get(1)?;
                Ok((role_str, content_json))
            })?
            .filter_map(|r| r.ok())
            .filter_map(|(role_str, content_json)| {
                let role = role_str.parse::<Role>().ok()?;
                let payload: StoredPayload = serde_json::from_str(&content_json).ok()?;
                Some(StoredMessage { role, payload })
            })
            .collect();

        Ok(messages)
    }

    async fn get_thread_messages_with_ancestry(
        &self,
        thread_id: &str,
    ) -> Result<Vec<StoredMessage>> {
        // First, build the ancestry chain by walking up parent_span_id references
        let mut ancestry_chain: Vec<(String, Option<String>)> = Vec::new();
        let mut current_thread_id = thread_id.to_string();

        loop {
            let thread = self.get_thread(&current_thread_id).await?;
            match thread {
                Some(t) => {
                    let parent_span_id = t.parent_span_id.clone();
                    ancestry_chain.push((current_thread_id.clone(), parent_span_id.clone()));

                    if let Some(ref span_id) = parent_span_id {
                        // Find which thread this span belongs to
                        if let Some(span_set_id) = self.get_span_parent_span_set(span_id).await? {
                            if let Some(parent_thread_id) =
                                self.get_span_set_thread(&span_set_id).await?
                            {
                                current_thread_id = parent_thread_id;
                                continue;
                            }
                        }
                    }
                    break;
                }
                None => break,
            }
        }

        // Reverse so we process from root (main thread) to leaf (current thread)
        ancestry_chain.reverse();

        let mut all_messages: Vec<StoredMessage> = Vec::new();

        for (i, (tid, _parent_span_id)) in ancestry_chain.iter().enumerate() {
            // Get span_sets for this thread
            let span_sets = self.get_thread_span_sets(tid).await?;

            for span_set_info in span_sets {
                // If this is not the first thread in ancestry, we need to check
                // if we should stop at the fork point
                if i < ancestry_chain.len() - 1 {
                    // Check if next thread forks from a span in this span_set
                    if let Some((_, Some(next_parent_span))) = ancestry_chain.get(i + 1) {
                        // Check if this span_set contains the fork point
                        let alternates = self.get_span_set_alternates(&span_set_info.id).await?;
                        let contains_fork_point =
                            alternates.iter().any(|a| &a.id == next_parent_span);

                        if contains_fork_point {
                            // Include this span_set (using the forked-from span) and stop
                            let messages = self.get_span_messages(next_parent_span).await?;
                            all_messages.extend(messages);
                            break;
                        }
                    }
                }

                // Include messages from the selected span of this span_set
                if let Some(span_set) = self.get_span_set_with_content(&span_set_info.id).await? {
                    all_messages.extend(span_set.messages);
                }
            }
        }

        Ok(all_messages)
    }

    // ========== Helper Methods ==========

    async fn add_user_span_set(&self, thread_id: &str, content: &StoredPayload) -> Result<String> {
        let span_set_id = self.create_span_set(thread_id, SpanType::User).await?;
        let span_id = self.create_span(&span_set_id, None).await?;
        self.add_span_message(&span_id, Role::User, content).await?;
        Ok(span_set_id)
    }

    async fn add_assistant_span_set(&self, thread_id: &str) -> Result<String> {
        self.create_span_set(thread_id, SpanType::Assistant).await
    }

    async fn add_assistant_span(
        &self,
        span_set_id: &str,
        model_id: &str,
        content: &StoredPayload,
    ) -> Result<String> {
        let span_id = self.create_span(span_set_id, Some(model_id)).await?;
        self.add_span_message(&span_id, Role::Assistant, content)
            .await?;
        Ok(span_id)
    }
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
            params![id.as_str(), conversation_id.as_str(), role.as_str(), sequence_number, now],
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
                let role = SpanRole::from_str(&role_str)?;
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
                let role = SpanRole::from_str(&role_str)
                    .ok_or_else(|| anyhow::anyhow!("Invalid role: {}", role_str))?;
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

    async fn add_span(
        &self,
        turn_id: &TurnId,
        model_id: Option<&str>,
    ) -> Result<SpanInfo> {
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
                let id: String = row.get(0)?;
                let tid: String = row.get(1)?;
                let model: Option<String> = row.get(2)?;
                let created: i64 = row.get(3)?;
                let msg_count: i32 = row.get(4)?;
                Ok((id, tid, model, created, msg_count))
            })?
            .filter_map(|r| r.ok())
            .map(|(id, tid, model, created, msg_count)| SpanInfo {
                id: SpanId::from_string(id),
                turn_id: TurnId::from_string(tid),
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
        message: NewMessage,
    ) -> Result<MessageInfo> {
        let conn = self.conn().lock().unwrap();
        let id = MessageId::new();
        let now = unix_timestamp();

        // Get next sequence number
        let sequence_number: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(sequence_number), -1) + 1 FROM messages WHERE span_id = ?1",
                params![span_id.as_str()],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Store text in content_blocks if present
        let content_id: Option<ContentBlockId> = if let Some(ref text) = message.text {
            let origin_kind = match message.role {
                MessageRole::User => Some("user"),
                MessageRole::Assistant => Some("assistant"),
                MessageRole::System => Some("system"),
                MessageRole::Tool => Some("system"),
            };
            let content_block_id = store_content_sync(&conn, text, origin_kind, None, None)?;
            Some(ContentBlockId::from_string(content_block_id))
        } else {
            None
        };

        conn.execute(
            "INSERT INTO messages (id, span_id, sequence_number, role, content_id, tool_calls, tool_results, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id.as_str(),
                span_id.as_str(),
                sequence_number,
                message.role.as_str(),
                content_id.as_ref().map(|c| c.as_str()),
                message.tool_calls,
                message.tool_results,
                now
            ],
        )?;

        Ok(MessageInfo {
            id,
            span_id: span_id.clone(),
            sequence_number,
            role: message.role,
            content_id,
            tool_calls: message.tool_calls,
            tool_results: message.tool_results,
            created_at: now,
        })
    }

    async fn get_messages(&self, span_id: &SpanId) -> Result<Vec<MessageInfo>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, span_id, sequence_number, role, content_id, tool_calls, tool_results, created_at
             FROM messages WHERE span_id = ?1
             ORDER BY sequence_number",
        )?;

        let messages = stmt
            .query_map(params![span_id.as_str()], |row| {
                let id: String = row.get(0)?;
                let sid: String = row.get(1)?;
                let seq: i32 = row.get(2)?;
                let role_str: String = row.get(3)?;
                let content_id: Option<String> = row.get(4)?;
                let tool_calls: Option<String> = row.get(5)?;
                let tool_results: Option<String> = row.get(6)?;
                let created: i64 = row.get(7)?;
                Ok((id, sid, seq, role_str, content_id, tool_calls, tool_results, created))
            })?
            .filter_map(|r| r.ok())
            .filter_map(|(id, sid, seq, role_str, content_id, tool_calls, tool_results, created)| {
                let role = MessageRole::from_str(&role_str)?;
                Some(MessageInfo {
                    id: MessageId::from_string(id),
                    span_id: SpanId::from_string(sid),
                    sequence_number: seq,
                    role,
                    content_id: content_id.map(ContentBlockId::from_string),
                    tool_calls,
                    tool_results,
                    created_at: created,
                })
            })
            .collect();

        Ok(messages)
    }

    async fn get_message(&self, message_id: &MessageId) -> Result<Option<MessageInfo>> {
        let conn = self.conn().lock().unwrap();
        let result = conn.query_row(
            "SELECT id, span_id, sequence_number, role, content_id, tool_calls, tool_results, created_at
             FROM messages WHERE id = ?1",
            params![message_id.as_str()],
            |row| {
                let id: String = row.get(0)?;
                let sid: String = row.get(1)?;
                let seq: i32 = row.get(2)?;
                let role_str: String = row.get(3)?;
                let content_id: Option<String> = row.get(4)?;
                let tool_calls: Option<String> = row.get(5)?;
                let tool_results: Option<String> = row.get(6)?;
                let created: i64 = row.get(7)?;
                Ok((id, sid, seq, role_str, content_id, tool_calls, tool_results, created))
            },
        );

        match result {
            Ok((id, sid, seq, role_str, content_id, tool_calls, tool_results, created)) => {
                let role = MessageRole::from_str(&role_str)
                    .ok_or_else(|| anyhow::anyhow!("Invalid role: {}", role_str))?;
                Ok(Some(MessageInfo {
                    id: MessageId::from_string(id),
                    span_id: SpanId::from_string(sid),
                    sequence_number: seq,
                    role,
                    content_id: content_id.map(ContentBlockId::from_string),
                    tool_calls,
                    tool_results,
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
            params![id.as_str(), conversation_id.as_str(), name, is_main as i32, now],
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
            .map(|(id, cid, name, is_main_int, forked_from, forked_at, created)| ViewInfo {
                id: ViewId::from_string(id),
                conversation_id: ConversationId::from_string(cid),
                name,
                is_main: is_main_int != 0,
                forked_from_view_id: forked_from.map(ViewId::from_string),
                forked_at_turn_id: forked_at.map(TurnId::from_string),
                created_at: created,
            })
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

    async fn select_span(
        &self,
        view_id: &ViewId,
        turn_id: &TurnId,
        span_id: &SpanId,
    ) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        // Upsert: insert or update selection
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
        // First get the conversation_id for this view (in a block to drop conn before await)
        let conversation_id = {
            let conn = self.conn().lock().unwrap();
            conn.query_row(
                "SELECT conversation_id FROM views WHERE id = ?1",
                params![view_id.as_str()],
                |row| row.get::<_, String>(0),
            )?
        };

        // Get all turns
        let turns = self.get_turns(&ConversationId::from_string(conversation_id)).await?;

        let mut result = Vec::new();
        for turn in turns {
            // Get selected span for this turn (or first span if none selected)
            let selected_span_id = self.get_selected_span(view_id, &turn.id).await?;

            let span = if let Some(span_id) = selected_span_id {
                self.get_span(&span_id).await?
            } else {
                // Get first span for this turn
                let spans = self.get_spans(&turn.id).await?;
                spans.into_iter().next()
            };

            if let Some(span) = span {
                let messages = self.get_messages(&span.id).await?;
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
        // Get the original view
        let conn = self.conn().lock().unwrap();
        let (conversation_id, _): (String, i32) = conn.query_row(
            "SELECT conversation_id, is_main FROM views WHERE id = ?1",
            params![view_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        drop(conn);

        // Create new view
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

        // Copy selections from original view up to (but not including) the fork turn
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
            conversation_id: ConversationId::from_string(conversation_id),
            name: name.map(|s| s.to_string()),
            is_main: false,
            forked_from_view_id: Some(view_id.clone()),
            forked_at_turn_id: Some(at_turn_id.clone()),
            created_at: now,
        })
    }

    // ========== Convenience Methods ==========

    async fn add_user_turn(
        &self,
        conversation_id: &ConversationId,
        text: &str,
    ) -> Result<(TurnInfo, SpanInfo, MessageInfo)> {
        let turn = self.add_turn(conversation_id, SpanRole::User).await?;
        let span = self.add_span(&turn.id, None).await?;
        let message = self.add_message(&span.id, NewMessage::user(text)).await?;

        // Auto-select this span in the main view
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
        let turn = self.add_turn(conversation_id, SpanRole::Assistant).await?;
        let span = self.add_span(&turn.id, Some(model_id)).await?;
        let message = self.add_message(&span.id, NewMessage::assistant(text)).await?;

        // Auto-select this span in the main view
        if let Some(main_view) = self.get_main_view(conversation_id).await? {
            self.select_span(&main_view.id, &turn.id, &span.id).await?;
        }

        Ok((turn, span, message))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_store() -> SqliteStore {
        // SqliteStore::in_memory() initializes all schemas
        SqliteStore::in_memory().unwrap()
    }

    fn create_test_conversation(store: &SqliteStore) -> ConversationId {
        let conn = store.conn().lock().unwrap();
        let id = ConversationId::new();
        let now = unix_timestamp();
        conn.execute(
            "INSERT INTO conversations (id, created_at, updated_at) VALUES (?1, ?2, ?2)",
            params![id.as_str(), now],
        ).unwrap();
        id
    }

    #[test]
    fn test_turn_schema_creation() {
        let conn = Connection::open_in_memory().unwrap();
        crate::storage::content_block::sqlite::init_schema(&conn).unwrap();
        init_schema(&conn).unwrap();

        // Verify turns table exists
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='turns'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Verify spans table exists
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='spans'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Verify messages table exists
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='messages'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_add_turn() {
        let store = create_test_store();
        let conv_id = create_test_conversation(&store);

        // Add a user turn
        let turn = store.add_turn(&conv_id, SpanRole::User).await.unwrap();
        assert_eq!(turn.sequence_number, 0);
        assert_eq!(turn.role, SpanRole::User);
        assert_eq!(turn.conversation_id, conv_id);

        // Add another turn
        let turn2 = store.add_turn(&conv_id, SpanRole::Assistant).await.unwrap();
        assert_eq!(turn2.sequence_number, 1);
        assert_eq!(turn2.role, SpanRole::Assistant);
    }

    #[tokio::test]
    async fn test_get_turns() {
        let store = create_test_store();
        let conv_id = create_test_conversation(&store);

        // Add multiple turns
        store.add_turn(&conv_id, SpanRole::User).await.unwrap();
        store.add_turn(&conv_id, SpanRole::Assistant).await.unwrap();
        store.add_turn(&conv_id, SpanRole::User).await.unwrap();

        // Get all turns
        let turns = store.get_turns(&conv_id).await.unwrap();
        assert_eq!(turns.len(), 3);
        assert_eq!(turns[0].sequence_number, 0);
        assert_eq!(turns[1].sequence_number, 1);
        assert_eq!(turns[2].sequence_number, 2);
    }

    #[tokio::test]
    async fn test_add_span() {
        let store = create_test_store();
        let conv_id = create_test_conversation(&store);

        let turn = store.add_turn(&conv_id, SpanRole::Assistant).await.unwrap();

        // Add a span with model
        let span = store.add_span(&turn.id, Some("claude-3")).await.unwrap();
        assert_eq!(span.turn_id, turn.id);
        assert_eq!(span.model_id.as_deref(), Some("claude-3"));
        assert_eq!(span.message_count, 0);
    }

    #[tokio::test]
    async fn test_add_message() {
        let store = create_test_store();
        let conv_id = create_test_conversation(&store);

        let turn = store.add_turn(&conv_id, SpanRole::User).await.unwrap();
        let span = store.add_span(&turn.id, None).await.unwrap();

        // Add a message
        let msg = store.add_message(&span.id, NewMessage::user("Hello!")).await.unwrap();
        assert_eq!(msg.sequence_number, 0);
        assert_eq!(msg.role, MessageRole::User);
        assert!(msg.content_id.is_some());

        // Add another message
        let msg2 = store.add_message(&span.id, NewMessage::user("Follow up")).await.unwrap();
        assert_eq!(msg2.sequence_number, 1);
    }

    #[tokio::test]
    async fn test_get_messages() {
        let store = create_test_store();
        let conv_id = create_test_conversation(&store);

        let turn = store.add_turn(&conv_id, SpanRole::Assistant).await.unwrap();
        let span = store.add_span(&turn.id, Some("claude-3")).await.unwrap();

        // Add messages
        store.add_message(&span.id, NewMessage::assistant("Thinking...")).await.unwrap();
        store.add_message(&span.id, NewMessage::assistant_with_tools(None, r#"{"tool": "search"}"#)).await.unwrap();
        store.add_message(&span.id, NewMessage::tool_result(r#"{"result": "found"}"#)).await.unwrap();
        store.add_message(&span.id, NewMessage::assistant("Here's what I found.")).await.unwrap();

        // Get all messages
        let messages = store.get_messages(&span.id).await.unwrap();
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, MessageRole::Assistant);
        assert_eq!(messages[1].role, MessageRole::Assistant);
        assert!(messages[1].tool_calls.is_some());
        assert_eq!(messages[2].role, MessageRole::Tool);
        assert!(messages[2].tool_results.is_some());
    }

    #[tokio::test]
    async fn test_multiple_spans_at_turn() {
        let store = create_test_store();
        let conv_id = create_test_conversation(&store);

        let turn = store.add_turn(&conv_id, SpanRole::Assistant).await.unwrap();

        // Add multiple spans (parallel model responses)
        let span1 = store.add_span(&turn.id, Some("claude-3")).await.unwrap();
        let span2 = store.add_span(&turn.id, Some("gpt-4")).await.unwrap();
        let span3 = store.add_span(&turn.id, Some("gemini")).await.unwrap();

        // Add different message counts to each
        store.add_message(&span1.id, NewMessage::assistant("Claude says hi")).await.unwrap();
        store.add_message(&span1.id, NewMessage::assistant("And more")).await.unwrap();

        store.add_message(&span2.id, NewMessage::assistant("GPT says hello")).await.unwrap();

        store.add_message(&span3.id, NewMessage::assistant("Gemini here")).await.unwrap();
        store.add_message(&span3.id, NewMessage::assistant("With tools")).await.unwrap();
        store.add_message(&span3.id, NewMessage::assistant("Done")).await.unwrap();

        // Get spans and verify message counts
        let spans = store.get_spans(&turn.id).await.unwrap();
        assert_eq!(spans.len(), 3);

        // Verify message counts match (refresh spans to get updated counts)
        let span1_fresh = store.get_span(&span1.id).await.unwrap().unwrap();
        let span2_fresh = store.get_span(&span2.id).await.unwrap().unwrap();
        let span3_fresh = store.get_span(&span3.id).await.unwrap().unwrap();

        assert_eq!(span1_fresh.message_count, 2);
        assert_eq!(span2_fresh.message_count, 1);
        assert_eq!(span3_fresh.message_count, 3);
    }

    #[tokio::test]
    async fn test_view_creation_and_selection() {
        let store = create_test_store();
        let conv_id = create_test_conversation(&store);

        // Create main view
        let view = store.create_view(&conv_id, Some("main"), true).await.unwrap();
        assert!(view.is_main);

        // Add turns and spans
        let turn1 = store.add_turn(&conv_id, SpanRole::User).await.unwrap();
        let span1 = store.add_span(&turn1.id, None).await.unwrap();
        store.add_message(&span1.id, NewMessage::user("Hello")).await.unwrap();

        let turn2 = store.add_turn(&conv_id, SpanRole::Assistant).await.unwrap();
        let span2a = store.add_span(&turn2.id, Some("claude-3")).await.unwrap();
        let span2b = store.add_span(&turn2.id, Some("gpt-4")).await.unwrap();
        store.add_message(&span2a.id, NewMessage::assistant("Claude response")).await.unwrap();
        store.add_message(&span2b.id, NewMessage::assistant("GPT response")).await.unwrap();

        // Select spans for view
        store.select_span(&view.id, &turn1.id, &span1.id).await.unwrap();
        store.select_span(&view.id, &turn2.id, &span2b.id).await.unwrap(); // Select GPT

        // Verify selection
        let selected = store.get_selected_span(&view.id, &turn2.id).await.unwrap();
        assert_eq!(selected, Some(span2b.id.clone()));

        // Get view path
        let path = store.get_view_path(&view.id).await.unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[1].span.model_id.as_deref(), Some("gpt-4"));
    }

    #[tokio::test]
    async fn test_convenience_methods() {
        let store = create_test_store();
        let conv_id = create_test_conversation(&store);

        // Create main view first
        store.create_view(&conv_id, None, true).await.unwrap();

        // Use convenience methods
        let (turn1, span1, msg1) = store.add_user_turn(&conv_id, "Hi there!").await.unwrap();
        assert_eq!(turn1.role, SpanRole::User);
        assert_eq!(msg1.role, MessageRole::User);

        let (turn2, span2, msg2) = store.add_assistant_turn(&conv_id, "claude-3", "Hello!").await.unwrap();
        assert_eq!(turn2.role, SpanRole::Assistant);
        assert_eq!(span2.model_id.as_deref(), Some("claude-3"));
        assert_eq!(msg2.role, MessageRole::Assistant);

        // Verify auto-selection in main view
        let main_view = store.get_main_view(&conv_id).await.unwrap().unwrap();
        let selected = store.get_selected_span(&main_view.id, &turn2.id).await.unwrap();
        assert_eq!(selected, Some(span2.id));
    }
}
