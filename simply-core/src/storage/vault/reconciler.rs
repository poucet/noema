//! Persist read-only vault reconciliation plans to projection tables.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::storage::helper::{content_hash, unix_timestamp};
use crate::storage::ids::{EntityId, VaultConflictId};
use crate::storage::traits::VaultStore;
use crate::storage::types::{VaultConflict, VaultConflictReason, VaultFile, VaultSyncStatus};
use crate::storage::vault::scanner::{
    ObservedVaultFile, VaultReconciliationAction, VaultScanOptions, normalize_relative_path,
    plan_reconciliation_with_options, plan_scoped_reconciliation_with_options, scan_vault,
    scan_vault_paths,
};
use crate::storage::vault::sidecar::{VaultSidecarManifest, write_sidecar_manifest};

/// Summary returned after persisting a reconciliation plan.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VaultReconciliationSummary {
    pub scanned_files: usize,
    pub actions: usize,
    pub projected_files: usize,
    pub conflicts: usize,
    pub missing_files: usize,
    pub unmanaged_files: usize,
}

/// Coordinates read-only scan output into vault projection storage.
pub struct VaultReconciler {
    root: PathBuf,
    store: Arc<dyn VaultStore>,
    options: VaultScanOptions,
}

impl VaultReconciler {
    pub fn new(root: PathBuf, store: Arc<dyn VaultStore>) -> Self {
        Self {
            root,
            store,
            options: VaultScanOptions::default(),
        }
    }

    pub fn with_options(mut self, options: VaultScanOptions) -> Self {
        self.options = options;
        self
    }

    /// Full startup-style scan. Missing projected files are marked missing.
    pub async fn reconcile_full_scan(&self) -> Result<VaultReconciliationSummary> {
        let known_files = self.store.list_vault_files(None).await?;
        let observed_files = scan_vault(&self.root, &self.options)?;
        let plan = plan_reconciliation_with_options(&known_files, &observed_files, &self.options);
        self.persist_plan(&known_files, &observed_files, plan.actions)
            .await
    }

    /// Scoped rescan for watcher/manual paths. Only projected files inside the
    /// provided scope can be marked missing.
    pub async fn reconcile_paths(&self, paths: &[PathBuf]) -> Result<VaultReconciliationSummary> {
        let known_files = self.store.list_vault_files(None).await?;
        let observed_files = scan_vault_paths(&self.root, paths, &self.options)?;
        let missing_paths = scoped_missing_paths(&self.root, paths);
        let plan = plan_scoped_reconciliation_with_options(
            &known_files,
            &observed_files,
            &missing_paths,
            &self.options,
        );
        self.persist_plan(&known_files, &observed_files, plan.actions)
            .await
    }

    async fn persist_plan(
        &self,
        known_files: &[VaultFile],
        observed_files: &[ObservedVaultFile],
        actions: Vec<VaultReconciliationAction>,
    ) -> Result<VaultReconciliationSummary> {
        let now = unix_timestamp();
        let known_by_entity: HashMap<_, _> = known_files
            .iter()
            .map(|file| (file.entity_id.as_str().to_string(), file))
            .collect();
        let observed_by_path: HashMap<_, _> = observed_files
            .iter()
            .map(|file| (file.path.as_str(), file))
            .collect();

        let mut summary = VaultReconciliationSummary {
            scanned_files: observed_files.len(),
            actions: actions.len(),
            ..Default::default()
        };

        for action in actions {
            match action {
                VaultReconciliationAction::KeepSynced { entity_id, path }
                | VaultReconciliationAction::ImportUnknown {
                    entity_id, path, ..
                } => {
                    if let Some(observed) = observed_by_path.get(path.as_str()) {
                        self.clear_path_conflicts(&path).await?;
                        self.upsert_synced_file(entity_id, observed, now).await?;
                        summary.projected_files += 1;
                    }
                }
                VaultReconciliationAction::UpdatePath {
                    entity_id,
                    from_path,
                    to_path,
                } => {
                    if let Some(observed) = observed_by_path.get(to_path.as_str()) {
                        self.clear_path_conflicts(&from_path).await?;
                        self.clear_path_conflicts(&to_path).await?;
                        self.upsert_synced_file(entity_id, observed, now).await?;
                        summary.projected_files += 1;
                    }
                }
                VaultReconciliationAction::MarkMissing { entity_id, .. } => {
                    if let Some(known) = known_by_entity.get(entity_id.as_str()) {
                        let mut missing = (**known).clone();
                        missing.sync_status = VaultSyncStatus::missing();
                        missing.last_seen_at = now;
                        self.store.upsert_vault_file(&missing).await?;
                        summary.missing_files += 1;
                    }
                }
                VaultReconciliationAction::Conflict {
                    path,
                    reason,
                    entity_id,
                    observed_entity_id,
                    details,
                } => {
                    self.clear_path_conflicts(&path).await?;
                    if let Some(entity_id) = entity_id.as_ref() {
                        if let Some(known) = known_by_entity.get(entity_id.as_str()) {
                            let mut conflicted = (**known).clone();
                            conflicted.sync_status = VaultSyncStatus::conflict();
                            conflicted.last_seen_at = now;
                            self.store.upsert_vault_file(&conflicted).await?;
                        }
                    }
                    self.insert_conflict(
                        &path,
                        reason,
                        entity_id,
                        observed_entity_id,
                        details,
                        now,
                    )
                    .await?;
                    summary.conflicts += 1;
                }
                VaultReconciliationAction::Unmanaged {
                    path,
                    reason,
                    details,
                } => {
                    self.clear_path_conflicts(&path).await?;
                    self.insert_conflict(&path, reason, None, None, details, now)
                        .await?;
                    summary.unmanaged_files += 1;
                }
            }
        }

        self.write_sidecar_snapshot().await?;
        Ok(summary)
    }

    async fn write_sidecar_snapshot(&self) -> Result<()> {
        let files = self.store.list_vault_files(None).await?;
        let manifest = VaultSidecarManifest::from_vault_files(&files);
        write_sidecar_manifest(&self.root, &manifest)
    }

    async fn upsert_synced_file(
        &self,
        entity_id: EntityId,
        observed: &ObservedVaultFile,
        now: i64,
    ) -> Result<()> {
        self.store
            .upsert_vault_file(&VaultFile {
                entity_id,
                path: observed.path.clone(),
                file_key: None,
                mtime: observed.mtime,
                content_hash: observed.content_hash.clone(),
                frontmatter_hash: observed.frontmatter_hash.clone(),
                sync_status: VaultSyncStatus::synced(),
                last_seen_at: now,
            })
            .await
    }

    async fn insert_conflict(
        &self,
        path: &str,
        reason: VaultConflictReason,
        entity_id: Option<EntityId>,
        observed_entity_id: Option<EntityId>,
        details: serde_json::Value,
        now: i64,
    ) -> Result<()> {
        self.store
            .insert_vault_conflict(&VaultConflict {
                id: stable_conflict_id(
                    path,
                    &reason,
                    entity_id.as_ref(),
                    observed_entity_id.as_ref(),
                ),
                entity_id,
                path: path.to_string(),
                reason,
                observed_entity_id,
                details: Some(details),
                created_at: now,
            })
            .await
    }

    async fn clear_path_conflicts(&self, path: &str) -> Result<()> {
        self.store.clear_vault_conflicts_for_path(path).await
    }
}

fn stable_conflict_id(
    path: &str,
    reason: &VaultConflictReason,
    entity_id: Option<&EntityId>,
    observed_entity_id: Option<&EntityId>,
) -> VaultConflictId {
    let key = format!(
        "{}:{}:{}:{}",
        path,
        reason.as_str(),
        entity_id.map(EntityId::as_str).unwrap_or(""),
        observed_entity_id.map(EntityId::as_str).unwrap_or("")
    );
    VaultConflictId::from_string(format!("vault_conflict_{}", content_hash(&key)))
}

fn scoped_missing_paths(root: &Path, paths: &[PathBuf]) -> HashSet<String> {
    paths
        .iter()
        .filter_map(|path| {
            let relative = if path.is_absolute() {
                path.strip_prefix(root).ok()?
            } else {
                path.as_path()
            };
            Some(normalize_relative_path(relative))
        })
        .collect()
}
