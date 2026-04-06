//! UserStore trait for user account management

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::UserId;
use crate::storage::types::{Keyed, User};

/// Stored representation of a user (ID + user data)
pub type StoredUser = Keyed<UserId, User>;

/// Trait for user storage operations
#[async_trait]
pub trait UserStore: Send + Sync {
    /// Get or create the default user for single-tenant mode
    async fn get_or_create_default_user(&self) -> Result<StoredUser>;

    /// Get user by ID
    async fn get_user_by_id(&self, id: &UserId) -> Result<Option<StoredUser>>;

    /// Get user by email
    async fn get_user_by_email(&self, email: &str) -> Result<Option<StoredUser>>;

    /// Get or create a user by email
    async fn get_or_create_user_by_email(&self, email: &str) -> Result<StoredUser>;

    /// List all users in the database
    async fn list_users(&self) -> Result<Vec<StoredUser>>;

    /// Create a user token (returns the opaque token string)
    async fn create_user_token(&self, user_id: &UserId) -> Result<String>;

    /// Resolve a token to a user ID (returns None if expired or invalid)
    async fn resolve_token(&self, token: &str) -> Result<Option<UserId>>;

    /// Revoke all tokens for a user
    async fn revoke_user_tokens(&self, user_id: &UserId) -> Result<()>;

    /// Map a Discord user ID to a UCM user
    async fn map_discord_user(&self, discord_user_id: &str, user_id: &UserId) -> Result<()>;

    /// Resolve a Discord user ID to a UCM user ID
    async fn resolve_discord_user(&self, discord_user_id: &str) -> Result<Option<UserId>>;
}
