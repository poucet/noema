//! In-memory UserStore implementation

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

use crate::storage::ids::UserId;
use crate::storage::traits::{StoredUser, UserStore};
use crate::storage::types::{Keyed, User};

/// In-memory user store for testing
#[derive(Debug, Default)]
pub struct MemoryUserStore {
    users: Mutex<HashMap<String, StoredUser>>,
    discord_mappings: Mutex<HashMap<String, UserId>>,
    default_user_id: Mutex<Option<UserId>>,
}

impl MemoryUserStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl UserStore for MemoryUserStore {
    async fn get_or_create_default_user(&self) -> Result<StoredUser> {
        let mut default_id = self.default_user_id.lock().unwrap();

        if let Some(ref id) = *default_id {
            let users = self.users.lock().unwrap();
            if let Some(user) = users.get(id.as_str()) {
                return Ok(user.clone());
            }
        }

        // Create default user with a default email
        let id = UserId::new();
        let user = User::new("default@localhost");
        let stored = Keyed::new(id.clone(), user);

        self.users.lock().unwrap().insert(id.as_str().to_string(), stored.clone());
        *default_id = Some(id);

        Ok(stored)
    }

    async fn get_user_by_email(&self, email: &str) -> Result<Option<StoredUser>> {
        let users = self.users.lock().unwrap();
        Ok(users.values().find(|u| u.email.as_deref() == Some(email)).cloned())
    }

    async fn get_or_create_user_by_email(&self, email: &str) -> Result<StoredUser> {
        if let Some(user) = self.get_user_by_email(email).await? {
            return Ok(user);
        }

        let id = UserId::new();
        let user = User::new(email);
        let stored = Keyed::new(id.clone(), user);

        self.users.lock().unwrap().insert(id.as_str().to_string(), stored.clone());

        Ok(stored)
    }

    async fn get_user_by_id(&self, id: &UserId) -> Result<Option<StoredUser>> {
        let users = self.users.lock().unwrap();
        Ok(users.get(id.as_str()).cloned())
    }

    async fn list_users(&self) -> Result<Vec<StoredUser>> {
        let users = self.users.lock().unwrap();
        Ok(users.values().cloned().collect())
    }

    async fn delete_user(&self, id: &UserId) -> Result<bool> {
        Ok(self.users.lock().unwrap().remove(id.as_str()).is_some())
    }

    async fn resolve_external_user(&self, external_id: &str) -> Result<Option<UserId>> {
        Ok(self.discord_mappings.lock().unwrap().get(external_id).cloned())
    }

    async fn resolve_or_create_external_user(&self, external_id: &str) -> Result<StoredUser> {
        // Check if already mapped
        if let Some(user_id) = self.resolve_external_user(external_id).await? {
            if let Some(user) = self.get_user_by_id(&user_id).await? {
                return Ok(user);
            }
        }

        // Create new user + mapping
        let id = UserId::new();
        let user = User::anonymous();
        let stored = Keyed::new(id.clone(), user);
        self.users.lock().unwrap().insert(id.as_str().to_string(), stored.clone());
        self.discord_mappings.lock().unwrap().insert(external_id.to_string(), id);
        Ok(stored)
    }

    async fn link_external_id(&self, user_id: &UserId, external_id: &str) -> Result<()> {
        self.discord_mappings.lock().unwrap()
            .insert(external_id.to_string(), user_id.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_default_user() {
        let store = MemoryUserStore::new();

        let user1 = store.get_or_create_default_user().await.unwrap();
        let user2 = store.get_or_create_default_user().await.unwrap();

        // Same user returned
        assert_eq!(user1.id, user2.id);
        assert_eq!(user1.email.as_deref(), Some("default@localhost"));
    }

    #[tokio::test]
    async fn test_get_or_create_by_email() {
        let store = MemoryUserStore::new();

        let user1 = store.get_or_create_user_by_email("test@example.com").await.unwrap();
        let user2 = store.get_or_create_user_by_email("test@example.com").await.unwrap();

        // Same user returned
        assert_eq!(user1.id, user2.id);
        assert_eq!(user1.email.as_deref(), Some("test@example.com"));
    }

    #[tokio::test]
    async fn test_list_users() {
        let store = MemoryUserStore::new();

        store.get_or_create_default_user().await.unwrap();
        store.get_or_create_user_by_email("user@example.com").await.unwrap();

        let users = store.list_users().await.unwrap();
        assert_eq!(users.len(), 2);
    }
}
