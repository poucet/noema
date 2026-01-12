//! SQLite implementation of ConversationStore

use anyhow::Result;
use async_trait::async_trait;
use rusqlite::{params, Connection};

use super::SqliteStore;
use crate::storage::helper::unix_timestamp;
use crate::storage::ids::{ConversationId, UserId};
use crate::storage::traits::ConversationStore;
use crate::storage::types::ConversationInfo;

/// Initialize conversation schema (conversations table)
pub(crate) fn init_schema(conn: &Connection) -> Result<()> {
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

        CREATE INDEX IF NOT EXISTS idx_conversations_user ON conversations(user_id);
        "#,
    )?;
    Ok(())
}

// ============================================================================
// ConversationStore Implementation
// ============================================================================

#[async_trait]
impl ConversationStore for SqliteStore {
    async fn list_conversations(&self, user_id: &UserId) -> Result<Vec<ConversationInfo>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT c.id, c.title, c.is_private, c.created_at, c.updated_at,
                    (SELECT COUNT(*) FROM turns t WHERE t.conversation_id = c.id) as turn_count
             FROM conversations c
             WHERE c.user_id = ?1
             ORDER BY c.updated_at DESC",
        )?;

        let conversations = stmt
            .query_map(params![user_id.as_str()], |row| {
                let id: String = row.get(0)?;
                let name: Option<String> = row.get(1)?;
                let is_private: i32 = row.get(2)?;
                let created_at: i64 = row.get(3)?;
                let updated_at: i64 = row.get(4)?;
                let turn_count: usize = row.get(5)?;
                Ok((id, name, is_private, created_at, updated_at, turn_count))
            })?
            .filter_map(|r| r.ok())
            .map(
                |(id, name, is_private, created_at, updated_at, turn_count)| ConversationInfo {
                    id: ConversationId::from_string(id),
                    name,
                    turn_count,
                    is_private: is_private != 0,
                    created_at,
                    updated_at,
                },
            )
            .collect();

        Ok(conversations)
    }

    async fn delete_conversation(&self, conversation_id: &ConversationId) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        // Cascade delete handles turns, spans, messages, message_content, views, view_selections
        conn.execute(
            "DELETE FROM conversations WHERE id = ?1",
            params![conversation_id.as_str()],
        )?;
        Ok(())
    }

    async fn rename_conversation(
        &self,
        conversation_id: &ConversationId,
        name: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        conn.execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, unix_timestamp(), conversation_id.as_str()],
        )?;
        Ok(())
    }

    async fn is_conversation_private(&self, conversation_id: &ConversationId) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        let is_private: i32 = conn.query_row(
            "SELECT is_private FROM conversations WHERE id = ?1",
            params![conversation_id.as_str()],
            |row| row.get(0),
        )?;
        Ok(is_private != 0)
    }

    async fn set_conversation_private(
        &self,
        conversation_id: &ConversationId,
        is_private: bool,
    ) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        conn.execute(
            "UPDATE conversations SET is_private = ?1, updated_at = ?2 WHERE id = ?3",
            params![is_private as i32, unix_timestamp(), conversation_id.as_str()],
        )?;
        Ok(())
    }
}
