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

    /// Resolve an external identity (e.g. "discord:123") to a UCM user ID.
    /// Returns None if the identity is not mapped.
    async fn resolve_external_user(&self, external_id: &str) -> Result<Option<UserId>>;

    /// Resolve an external identity to a UCM user, creating both the user
    /// and the mapping if they don't exist yet. Idempotent — same external_id
    /// always returns the same user. A single user can have multiple external IDs.
    async fn resolve_or_create_external_user(&self, external_id: &str) -> Result<StoredUser>;
}
