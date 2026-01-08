//! SQLite implementation of ConversationStore

use anyhow::{Context, Result};
use async_trait::async_trait;
use llm::api::Role;
use rusqlite::{params, Connection};
use uuid::Uuid;

use super::{
    ConversationInfo, ConversationStore, SpanInfo, SpanSetInfo, SpanSetWithContent, SpanType,
    ThreadInfo,
};
use crate::storage::content::{StoredMessage, StoredPayload};
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

        -- Spans: alternative responses within a SpanSet
        CREATE TABLE IF NOT EXISTS spans (
            id TEXT PRIMARY KEY,
            span_set_id TEXT REFERENCES span_sets(id) ON DELETE CASCADE,
            model_id TEXT,
            created_at INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_spans_span_set ON spans(span_set_id);

        -- Span messages: individual messages within a span (for multi-turn agentic responses)
        CREATE TABLE IF NOT EXISTS span_messages (
            id TEXT PRIMARY KEY,
            span_id TEXT REFERENCES spans(id) ON DELETE CASCADE,
            sequence_number INTEGER NOT NULL,
            role TEXT CHECK(role IN ('user', 'assistant', 'system', 'tool')) NOT NULL,
            content TEXT NOT NULL,
            text_content TEXT,
            created_at INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_span_messages_span ON span_messages(span_id, sequence_number);
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
        let query = "SELECT c.id, c.title, COUNT(ss.id) as msg_count, c.created_at, c.updated_at
             FROM conversations c
             LEFT JOIN threads t ON t.conversation_id = c.id AND t.parent_span_id IS NULL
             LEFT JOIN span_sets ss ON ss.thread_id = t.id
             WHERE c.user_id = ?1
             GROUP BY c.id
             ORDER BY c.updated_at DESC";

        let mut stmt = conn.prepare(query)?;
        let infos: Vec<ConversationInfo> = stmt
            .query_map(params![user_id], |row| {
                Ok(ConversationInfo {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    message_count: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
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
             FROM span_messages sm
             JOIN spans s ON sm.span_id = s.id
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
                "SELECT span_set_id FROM spans WHERE id = ?1",
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

    async fn get_span_set_alternates(&self, span_set_id: &str) -> Result<Vec<SpanInfo>> {
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
                    (SELECT COUNT(*) FROM span_messages WHERE span_id = s.id) as msg_count
             FROM spans s
             WHERE s.span_set_id = ?1
             ORDER BY s.created_at",
        )?;

        let spans = stmt
            .query_map(params![span_set_id], |row| {
                let id: String = row.get(0)?;
                Ok(SpanInfo {
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
            "INSERT INTO spans (id, span_set_id, model_id, created_at)
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
                "SELECT COALESCE(MAX(sequence_number), 0) + 1 FROM span_messages WHERE span_id = ?1",
                params![span_id],
                |row| row.get(0),
            )
            .unwrap_or(1);

        let content_json = serde_json::to_string(content)?;

        conn.execute(
            "INSERT INTO span_messages (id, span_id, sequence_number, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![&id, span_id, sequence_number, role.to_string(), &content_json, now],
        )?;

        Ok(id)
    }

    async fn get_span_messages(&self, span_id: &str) -> Result<Vec<StoredMessage>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT role, content FROM span_messages WHERE span_id = ?1 ORDER BY sequence_number",
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
