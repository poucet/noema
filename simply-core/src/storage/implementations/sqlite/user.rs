//! SQLite implementation of UserStore

use anyhow::{Context, Result};
use async_trait::async_trait;
use rusqlite::{params, Connection};

use super::SqliteStore;
use crate::storage::helper::unix_timestamp;
use crate::storage::ids::UserId;
use crate::storage::traits::{StoredUser, UserStore};
use crate::storage::types::{Keyed, User};

/// Default user email for single-tenant local mode
pub const DEFAULT_USER_EMAIL: &str = "human@noema";

pub (crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Users
        CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            email TEXT UNIQUE,
            encrypted_anthropic_key TEXT,
            encrypted_openai_key TEXT,
            encrypted_gemini_key TEXT,
            google_oauth_refresh_token TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        -- Discord user mapping (discord_user_id → UCM user)
        CREATE TABLE IF NOT EXISTS discord_user_mappings (
            discord_user_id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_discord_mappings_user ON discord_user_mappings(user_id);
        "#,
    )
    .context("Failed to initialize user schema")?;
    Ok(())
}

#[async_trait]
impl UserStore for SqliteStore {
    async fn get_or_create_default_user(&self) -> Result<StoredUser> {
        let conn = self.conn().lock().unwrap();

        // Try to get existing user
        let user: Option<StoredUser> = conn
            .query_row(
                "SELECT id, email FROM users WHERE email = ?1",
                params![DEFAULT_USER_EMAIL],
                |row| {
                    Ok(Keyed::new(
                        row.get::<_, UserId>(0)?,
                        User { email: row.get::<_, Option<String>>(1)? },
                    ))
                },
            )
            .ok();

        if let Some(u) = user {
            return Ok(u);
        }

        // Create default user
        let id = UserId::new();
        let now = unix_timestamp();
        conn.execute(
            "INSERT INTO users (id, email, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![id.as_str(), DEFAULT_USER_EMAIL, now, now],
        )?;

        Ok(Keyed::new(id, User::new(DEFAULT_USER_EMAIL)))
    }

    async fn get_user_by_email(&self, email: &str) -> Result<Option<StoredUser>> {
        let conn = self.conn().lock().unwrap();
        let user = conn
            .query_row(
                "SELECT id, email FROM users WHERE email = ?1",
                params![email],
                |row| {
                    Ok(Keyed::new(
                        row.get::<_, UserId>(0)?,
                        User { email: row.get::<_, Option<String>>(1)? },
                    ))
                },
            )
            .ok();
        Ok(user)
    }

    async fn get_or_create_user_by_email(&self, email: &str) -> Result<StoredUser> {
        // Try to get existing user first
        if let Some(user) = self.get_user_by_email(email).await? {
            return Ok(user);
        }

        // Create new user
        let conn = self.conn().lock().unwrap();
        let id = UserId::new();
        let now = unix_timestamp();

        conn.execute(
            "INSERT INTO users (id, email, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![id.as_str(), email, now, now],
        )?;

        Ok(Keyed::new(id, User::new(email)))
    }

    async fn get_user_by_id(&self, id: &UserId) -> Result<Option<StoredUser>> {
        let conn = self.conn().lock().unwrap();
        let user = conn
            .query_row(
                "SELECT id, email FROM users WHERE id = ?1",
                params![id.as_str()],
                |row| {
                    Ok(Keyed::new(
                        row.get::<_, UserId>(0)?,
                        User { email: row.get::<_, Option<String>>(1)? },
                    ))
                },
            )
            .ok();
        Ok(user)
    }

    async fn list_users(&self) -> Result<Vec<StoredUser>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, email FROM users ORDER BY created_at")?;
        let users = stmt
            .query_map([], |row| {
                Ok(Keyed::new(
                    row.get::<_, UserId>(0)?,
                    User { email: row.get::<_, Option<String>>(1)? },
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(users)
    }

    async fn delete_user(&self, id: &UserId) -> Result<bool> {
        let conn = self.conn().lock().unwrap();
        conn.execute(
            "DELETE FROM discord_user_mappings WHERE user_id = ?1",
            params![id.as_str()],
        )?;
        let rows = conn.execute("DELETE FROM users WHERE id = ?1", params![id.as_str()])?;
        Ok(rows > 0)
    }

    async fn resolve_external_user(&self, external_id: &str) -> Result<Option<UserId>> {
        let conn = self.conn().lock().unwrap();
        let user_id = conn
            .query_row(
                "SELECT user_id FROM discord_user_mappings WHERE discord_user_id = ?1",
                params![external_id],
                |row| row.get::<_, UserId>(0),
            )
            .ok();
        Ok(user_id)
    }

    async fn resolve_or_create_external_user(&self, external_id: &str) -> Result<StoredUser> {
        // Check if already mapped
        if let Some(user_id) = self.resolve_external_user(external_id).await? {
            if let Some(user) = self.get_user_by_id(&user_id).await? {
                return Ok(user);
            }
        }

        // Create new user + mapping atomically
        let conn = self.conn().lock().unwrap();
        let id = UserId::new();
        let now = unix_timestamp();

        conn.execute(
            "INSERT INTO users (id, email, created_at, updated_at) VALUES (?1, NULL, ?2, ?3)",
            params![id.as_str(), now, now],
        )?;

        conn.execute(
            "INSERT OR REPLACE INTO discord_user_mappings (discord_user_id, user_id) VALUES (?1, ?2)",
            params![external_id, id.as_str()],
        )?;

        Ok(Keyed::new(id, User::anonymous()))
    }
}
