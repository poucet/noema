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
        Ok(users.values().find(|u| u.email == email).cloned())
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

    async fn list_users(&self) -> Result<Vec<StoredUser>> {
        let users = self.users.lock().unwrap();
        Ok(users.values().cloned().collect())
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
        assert_eq!(user1.email, "default@localhost");
    }

    #[tokio::test]
    async fn test_get_or_create_by_email() {
        let store = MemoryUserStore::new();

        let user1 = store.get_or_create_user_by_email("test@example.com").await.unwrap();
        let user2 = store.get_or_create_user_by_email("test@example.com").await.unwrap();

        // Same user returned
        assert_eq!(user1.id, user2.id);
        assert_eq!(user1.email, "test@example.com");
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
