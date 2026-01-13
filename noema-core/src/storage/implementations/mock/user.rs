//! Mock user store for testing

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::traits::UserStore;
use crate::storage::types::UserInfo;

/// Mock user store that returns unimplemented for all operations
pub struct MockUserStore;

#[async_trait]
impl UserStore for MockUserStore {
    async fn get_or_create_default_user(&self) -> Result<UserInfo> {
        unimplemented!()
    }
    async fn get_user_by_email(&self, _: &str) -> Result<Option<UserInfo>> {
        unimplemented!()
    }
    async fn get_or_create_user_by_email(&self, _: &str) -> Result<UserInfo> {
        unimplemented!()
    }
    async fn list_users(&self) -> Result<Vec<UserInfo>> {
        unimplemented!()
    }
}
