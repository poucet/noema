//! Vault projection storage trait.

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::{EntityId, VaultConflictId};
use crate::storage::types::{VaultConflict, VaultFile, VaultSyncStatus};

/// Storage operations for Markdown vault projection and conflicts.
#[async_trait]
pub trait VaultStore: Send + Sync {
    async fn upsert_vault_file(&self, file: &VaultFile) -> Result<()>;

    async fn get_vault_file(&self, entity_id: &EntityId) -> Result<Option<VaultFile>>;

    async fn get_vault_file_by_path(&self, path: &str) -> Result<Option<VaultFile>>;

    async fn list_vault_files(&self, status: Option<&VaultSyncStatus>) -> Result<Vec<VaultFile>>;

    async fn delete_vault_file(&self, entity_id: &EntityId) -> Result<()>;

    async fn insert_vault_conflict(&self, conflict: &VaultConflict) -> Result<()>;

    async fn list_vault_conflicts(&self) -> Result<Vec<VaultConflict>>;

    async fn delete_vault_conflict(&self, id: &VaultConflictId) -> Result<bool>;

    async fn clear_vault_conflicts_for_path(&self, path: &str) -> Result<()>;
}
