//! Mock vault store for testing.

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::ids::{EntityId, VaultConflictId};
use crate::storage::traits::VaultStore;
use crate::storage::types::{VaultConflict, VaultFile, VaultSyncStatus};

pub struct MockVaultStore;

#[async_trait]
impl VaultStore for MockVaultStore {
    async fn upsert_vault_file(&self, _: &VaultFile) -> Result<()> {
        unimplemented!()
    }

    async fn get_vault_file(&self, _: &EntityId) -> Result<Option<VaultFile>> {
        unimplemented!()
    }

    async fn get_vault_file_by_path(&self, _: &str) -> Result<Option<VaultFile>> {
        unimplemented!()
    }

    async fn list_vault_files(&self, _: Option<&VaultSyncStatus>) -> Result<Vec<VaultFile>> {
        unimplemented!()
    }

    async fn delete_vault_file(&self, _: &EntityId) -> Result<()> {
        unimplemented!()
    }

    async fn insert_vault_conflict(&self, _: &VaultConflict) -> Result<()> {
        unimplemented!()
    }

    async fn list_vault_conflicts(&self) -> Result<Vec<VaultConflict>> {
        unimplemented!()
    }

    async fn delete_vault_conflict(&self, _: &VaultConflictId) -> Result<bool> {
        unimplemented!()
    }

    async fn clear_vault_conflicts_for_path(&self, _: &str) -> Result<()> {
        unimplemented!()
    }
}
