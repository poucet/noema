//! Mock user store for testing

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::traits::{StoredUser, UserStore};

/// Mock user store that returns unimplemented for all operations
pub struct MockUserStore;

#[async_trait]
impl UserStore for MockUserStore {
    async fn get_or_create_default_user(&self) -> Result<StoredUser> {
        unimplemented!()
    }
    async fn get_user_by_email(&self, _: &str) -> Result<Option<StoredUser>> {
        unimplemented!()
    }
    async fn get_or_create_user_by_email(&self, _: &str) -> Result<StoredUser> {
        unimplemented!()
    }
    async fn list_users(&self) -> Result<Vec<StoredUser>> {
        unimplemented!()
    }
}
