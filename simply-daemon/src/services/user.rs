//! User identity service.

use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::Mutex;
use simply_core::storage::coordinator::StorageCoordinator;
use simply_core::storage::traits::{StorageTypes, Stores};
use simply_rpc::RequestContext;
use crate::api::*;

pub struct UserService<S: StorageTypes> {
    stores: Arc<dyn Stores<S>>,
}

impl<S: StorageTypes> UserService<S> {
    pub fn new(stores: Arc<dyn Stores<S>>) -> Self {
        Self { stores }
    }
}

#[async_trait]
impl<S: StorageTypes> UserApi for UserService<S>
where
    S::User: simply_core::storage::traits::UserStore,
{
    async fn resolve_user(&self, _ctx: &RequestContext, external_id: String) -> anyhow::Result<Option<simply_rpc::Scope>> {
        use simply_core::storage::traits::UserStore;
        let user_store = self.stores.user();
        match user_store.resolve_external_user(&external_id).await? {
            Some(user_id) => Ok(Some(simply_rpc::Scope::user(user_id.as_str()))),
            None => Ok(None),
        }
    }

    async fn resolve_or_create_user(&self, _ctx: &RequestContext, external_id: String) -> anyhow::Result<simply_rpc::Scope> {
        use simply_core::storage::traits::UserStore;
        let user = self.stores.user().resolve_or_create_external_user(&external_id).await?;
        Ok(simply_rpc::Scope::user(user.id.as_str()))
    }
}

