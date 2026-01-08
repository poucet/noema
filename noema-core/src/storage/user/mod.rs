//! User storage trait and implementations
//!
//! Provides the `UserStore` trait for managing user accounts.

use anyhow::Result;
use async_trait::async_trait;

/// Information about a user
#[derive(Debug, Clone)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
}

/// Trait for user storage operations
#[async_trait]
pub trait UserStore: Send + Sync {
    /// Get or create the default user for single-tenant mode
    async fn get_or_create_default_user(&self) -> Result<UserInfo>;

    /// Get user by email
    async fn get_user_by_email(&self, email: &str) -> Result<Option<UserInfo>>;

    /// Get or create a user by email
    async fn get_or_create_user_by_email(&self, email: &str) -> Result<UserInfo>;

    /// List all users in the database
    async fn list_users(&self) -> Result<Vec<UserInfo>>;
}

#[cfg(feature = "sqlite")]
pub (crate) mod sqlite;
