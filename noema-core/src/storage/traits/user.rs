//! UserStore trait for user account management

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::types::user::UserInfo;

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
