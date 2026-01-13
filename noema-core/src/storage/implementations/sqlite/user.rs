//! SQLite implementation of UserStore

use anyhow::{Context, Result};
use async_trait::async_trait;
use rusqlite::{params, Connection};
use uuid::Uuid;

use super::SqliteStore;
use crate::storage::helper::unix_timestamp;
use crate::storage::ids::UserId;
use crate::storage::traits::UserStore;
use crate::storage::types::UserInfo;

/// Default user email for single-tenant local mode
pub const DEFAULT_USER_EMAIL: &str = "human@noema";

pub (crate) fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Users
        CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            email TEXT UNIQUE NOT NULL,
            encrypted_anthropic_key TEXT,
            encrypted_openai_key TEXT,
            encrypted_gemini_key TEXT,
            google_oauth_refresh_token TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        "#,
    )
    .context("Failed to initialize user schema")?;
    Ok(())
}

#[async_trait]
impl UserStore for SqliteStore {
    async fn get_or_create_default_user(&self) -> Result<UserInfo> {
        let conn = self.conn().lock().unwrap();

        // Try to get existing user
        let user: Option<UserInfo> = conn
            .query_row(
                "SELECT id, email FROM users WHERE email = ?1",
                params![DEFAULT_USER_EMAIL],
                |row| {
                    Ok(UserInfo {
                        id: row.get::<_, UserId>(0)?,
                        email: row.get(1)?,
                    })
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

        Ok(UserInfo {
            id,
            email: DEFAULT_USER_EMAIL.to_string(),
        })
    }

    async fn get_user_by_email(&self, email: &str) -> Result<Option<UserInfo>> {
        let conn = self.conn().lock().unwrap();
        let user = conn
            .query_row(
                "SELECT id, email FROM users WHERE email = ?1",
                params![email],
                |row| {
                    Ok(UserInfo {
                        id: row.get::<_, UserId>(0)?,
                        email: row.get(1)?,
                    })
                },
            )
            .ok();
        Ok(user)
    }

    async fn get_or_create_user_by_email(&self, email: &str) -> Result<UserInfo> {
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

        Ok(UserInfo {
            id,
            email: email.to_string(),
        })
    }

    async fn list_users(&self) -> Result<Vec<UserInfo>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, email FROM users ORDER BY created_at")?;
        let users = stmt
            .query_map([], |row| {
                Ok(UserInfo {
                    id: row.get::<_, UserId>(0)?,
                    email: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(users)
    }
}
