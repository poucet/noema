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

    /// Get user by email
    async fn get_user_by_email(&self, email: &str) -> Result<Option<StoredUser>>;

    /// Get or create a user by email
    async fn get_or_create_user_by_email(&self, email: &str) -> Result<StoredUser>;

    /// List all users in the database
    async fn list_users(&self) -> Result<Vec<StoredUser>>;
}
