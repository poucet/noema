//! User identity service.

use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::RwLock;
use simply_core::storage::ids::UserId;
use simply_core::storage::traits::{StorageTypes, Stores};
use simply_rpc::RequestContext;
use crate::api::*;

/// Global user_id → email cache.
///
/// Emails don't change after a user is created (at least not via our APIs),
/// and `Some(email)` / `None` are both worth caching — so we never invalidate.
/// Populated lazily on first lookup.
#[derive(Default)]
pub struct UserEmailCache {
    map: RwLock<HashMap<String, Option<String>>>,
}

impl UserEmailCache {
    pub fn new() -> Self { Self::default() }

    /// Look up the email for a user_id, caching the result.
    /// Returns None for unknown users and for users with no email set.
    pub async fn get<S>(&self, stores: &Arc<dyn Stores<S>>, user_id: &UserId) -> Option<String>
    where
        S: StorageTypes,
        S::User: simply_core::storage::traits::UserStore,
    {
        let key = user_id.as_str().to_string();
        if let Some(cached) = self.map.read().await.get(&key) {
            return cached.clone();
        }
        use simply_core::storage::traits::UserStore;
        let email = stores.user().get_user_by_id(user_id).await
            .ok()
            .flatten()
            .and_then(|u| u.content.email);
        self.map.write().await.insert(key, email.clone());
        email
    }

    /// Invalidate a single entry (e.g. after an email change or user delete).
    pub async fn invalidate(&self, user_id: &UserId) {
        self.map.write().await.remove(user_id.as_str());
    }
}

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

