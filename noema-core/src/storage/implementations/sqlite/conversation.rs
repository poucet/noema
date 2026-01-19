//! SQLite implementation of ConversationStore

use anyhow::Result;
use async_trait::async_trait;
use rusqlite::{params, Connection};

use super::SqliteStore;
use crate::storage::helper::unix_timestamp;
use crate::storage::ids::{ConversationId, UserId, ViewId};
use crate::storage::traits::ConversationStore;
use crate::storage::types::{stored, Conversation, Stored};

/// Initialize conversation schema (conversations table)
pub(crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Conversations
        CREATE TABLE IF NOT EXISTS conversations (
            id TEXT PRIMARY KEY,
            user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
            title TEXT,
            main_view_id TEXT REFERENCES views(id),
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
    async fn create_conversation(
        &self,
        user_id: &UserId,
        name: Option<&str>,
    ) -> Result<ConversationId> {
        let conn = self.conn().lock().unwrap();
        let now = unix_timestamp();
        let conv_id = ConversationId::new();

        // Create conversation record (coordinator will create view and set main_view_id)
        conn.execute(
            "INSERT INTO conversations (id, user_id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![conv_id.as_str(), user_id.as_str(), name, now, now],
        )?;

        Ok(conv_id)
    }

    async fn get_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Option<Stored<ConversationId, Conversation>>> {
        // Read from entities table (Entity Layer migration)
        let conn = self.conn().lock().unwrap();
        let result = conn.query_row(
            "SELECT id, name, metadata, is_private, created_at
             FROM entities WHERE id = ?1 AND entity_type = 'conversation'",
            params![conversation_id.as_str()],
            |row| {
                let id: ConversationId = row.get(0)?;
                let name: Option<String> = row.get(1)?;
                let metadata: Option<String> = row.get(2)?;
                let is_private: i32 = row.get(3)?;
                let created_at: i64 = row.get(4)?;
                Ok((id, name, metadata, is_private, created_at))
            },
        );

        match result {
            Ok((id, name, metadata, is_private, created_at)) => {
                // Extract main_view_id from metadata
                let metadata_json: Option<serde_json::Value> = metadata
                    .and_then(|m| serde_json::from_str(&m).ok());
                let main_view_id_str = metadata_json
                    .as_ref()
                    .and_then(|m| m.get("main_view_id"))
                    .and_then(|v| v.as_str());

                if let Some(view_id_str) = main_view_id_str {
                    let main_view_id = ViewId::from_string(view_id_str.to_string());
                    let conversation = Conversation {
                        name,
                        main_view_id,
                        is_private: is_private != 0,
                    };
                    Ok(Some(stored(id, conversation, created_at)))
                } else {
                    // Entity exists but doesn't have main_view_id yet
                    Ok(None)
                }
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn list_conversations(&self, user_id: &UserId) -> Result<Vec<Stored<ConversationId, Conversation>>> {
        // Read from entities table (Entity Layer migration)
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, metadata, is_private, created_at
             FROM entities
             WHERE user_id = ?1 AND entity_type = 'conversation' AND is_archived = 0
             ORDER BY updated_at DESC",
        )?;

        let conversations = stmt
            .query_map(params![user_id.as_str()], |row| {
                let id: ConversationId = row.get(0)?;
                let name: Option<String> = row.get(1)?;
                let metadata: Option<String> = row.get(2)?;
                let is_private: i32 = row.get(3)?;
                let created_at: i64 = row.get(4)?;
                Ok((id, name, metadata, is_private, created_at))
            })?
            .filter_map(|r| r.ok())
            .filter_map(|(id, name, metadata, is_private, created_at)| {
                // Extract main_view_id from metadata
                let metadata_json: Option<serde_json::Value> = metadata
                    .and_then(|m| serde_json::from_str(&m).ok());
                let main_view_id_str = metadata_json
                    .as_ref()
                    .and_then(|m| m.get("main_view_id"))
                    .and_then(|v| v.as_str())?;

                let main_view_id = ViewId::from_string(main_view_id_str.to_string());
                let conversation = Conversation {
                    name,
                    main_view_id,
                    is_private: is_private != 0,
                };
                Some(stored(id, conversation, created_at))
            })
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

    async fn set_main_view_id(
        &self,
        conversation_id: &ConversationId,
        view_id: &ViewId,
    ) -> Result<()> {
        let conn = self.conn().lock().unwrap();
        conn.execute(
            "UPDATE conversations SET main_view_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![view_id.as_str(), unix_timestamp(), conversation_id.as_str()],
        )?;
        Ok(())
    }
}
