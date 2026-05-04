//! Markdown vault service.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value};
use simply_core::storage::coordinator::StorageCoordinator;
use simply_core::storage::helper::{content_hash, unix_timestamp};
use simply_core::storage::ids::{AssetId as CoreAssetId, EntityId, UserId, VaultConflictId};
use simply_core::storage::traits::{
    AssetStore, EntityStore, StorageTypes, StoredEntity, Stores, TextStore, VaultStore,
};
use simply_core::storage::types::{
    BlobHash, ContentOrigin, Entity, EntityType, RelationType, VaultFile, VaultSyncStatus,
};
use simply_core::storage::vault::files::{
    numbered_path_segment, read_markdown_body, read_markdown_text, sanitize_path_segment,
    write_frontmatter_markdown, write_plain_markdown, WrittenMarkdownFile,
};
use simply_core::storage::vault::markdown::{
    parse_markdown, split_markdown, Frontmatter, SystemFrontmatter,
};
use simply_core::storage::vault::reconciler::{VaultReconciler, VaultReconciliationSummary};
use simply_core::storage::vault::scanner::{scan_vault, VaultScanOptions};
use simply_core::storage::vault::sidecar::{
    read_sidecar_manifest, write_sidecar_manifest, VaultSidecarFile, VaultSidecarManifest,
};
use simply_rpc::RequestContext;

use crate::api::*;

#[derive(Clone, Debug)]
struct SidecarMetadata {
    kind: EntityType,
    title: Option<String>,
    origin: Option<String>,
    parent_entity_id: Option<EntityId>,
    parent_relation: Option<RelationType>,
    position: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VaultFileFingerprint {
    mtime: Option<i64>,
    content_hash: String,
    frontmatter_hash: Option<String>,
}

pub struct VaultService<S: StorageTypes> {
    root: PathBuf,
    coordinator: Arc<StorageCoordinator<S>>,
    stores: Arc<dyn Stores<S>>,
    embedding_queue: Option<Arc<dyn crate::services::embedding_queue::EmbeddingQueue>>,
}

impl<S: StorageTypes> VaultService<S> {
    pub fn new(
        root: PathBuf,
        coordinator: Arc<StorageCoordinator<S>>,
        stores: Arc<dyn Stores<S>>,
    ) -> Self {
        Self {
            root,
            coordinator,
            stores,
            embedding_queue: None,
        }
    }

    pub fn with_embedding(
        mut self,
        queue: Arc<dyn crate::services::embedding_queue::EmbeddingQueue>,
    ) -> Self {
        self.embedding_queue = Some(queue);
        self
    }

    pub fn spawn_polling_watcher(self: Arc<Self>) {
        tokio::spawn(async move {
            if let Err(e) = self.scan_and_project().await {
                tracing::warn!(error = %e, "vault startup scan failed");
            }

            let mut snapshot = match self.vault_snapshot() {
                Ok(snapshot) => snapshot,
                Err(e) => {
                    tracing::warn!(error = %e, "vault watcher snapshot failed");
                    HashMap::new()
                }
            };

            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            loop {
                interval.tick().await;
                let next_snapshot = match self.vault_snapshot() {
                    Ok(snapshot) => snapshot,
                    Err(e) => {
                        tracing::warn!(error = %e, "vault watcher snapshot failed");
                        continue;
                    }
                };
                let mut changed_paths = changed_snapshot_paths(&snapshot, &next_snapshot);
                if changed_paths.is_empty() {
                    snapshot = next_snapshot;
                    continue;
                }

                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let settled_snapshot = match self.vault_snapshot() {
                    Ok(snapshot) => snapshot,
                    Err(e) => {
                        tracing::warn!(error = %e, "vault watcher debounce snapshot failed");
                        snapshot = next_snapshot;
                        continue;
                    }
                };
                changed_paths = changed_snapshot_paths(&snapshot, &settled_snapshot);
                snapshot = settled_snapshot;
                if changed_paths.is_empty() {
                    continue;
                }

                if let Err(e) = self.scan_paths_and_project(&changed_paths).await {
                    tracing::warn!(error = %e, "vault watcher path scan failed");
                }
            }
        });
    }

    fn require_user(ctx: &RequestContext) -> anyhow::Result<UserId> {
        ctx.scope
            .user_id
            .as_ref()
            .map(UserId::from_string)
            .ok_or_else(|| anyhow::anyhow!("authentication required"))
    }

    async fn verify_access(
        &self,
        user_id: &UserId,
        entity_id: &EntityId,
    ) -> anyhow::Result<StoredEntity> {
        let entity = self
            .stores
            .entity()
            .get_entity(entity_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("entity not found: {entity_id}"))?;
        match entity.user_id.as_ref() {
            Some(uid) if uid == user_id => Ok(entity),
            None => Ok(entity),
            Some(_) => anyhow::bail!("access denied: entity belongs to another user"),
        }
    }

    async fn export_roots(
        &self,
        user_id: &UserId,
        request: VaultExportRequest,
    ) -> anyhow::Result<VaultExportSummary> {
        let roots = if request.entity_ids.is_empty() {
            let docs = self
                .stores
                .entity()
                .list_entities_by_type_prefix(user_id, "document::")
                .await?;
            self.filter_roots(docs).await?
        } else {
            let ids: Vec<_> = request
                .entity_ids
                .iter()
                .map(|id| EntityId::from_string(id.as_str()))
                .collect();
            self.stores.entity().get_entities(&ids).await?
        };

        let mut summary = VaultExportSummary::default();
        let mut sidecar_metadata = HashMap::new();
        for root in roots {
            self.verify_access(user_id, &root.id).await?;
            if root.entity_type.as_str() == "document::tabbed" {
                let dir = self.root_dir_for(&root).await?;
                let files = self
                    .export_tab_tree(
                        &root,
                        &dir,
                        None,
                        None,
                        request.include_frontmatter_identity,
                        &mut sidecar_metadata,
                    )
                    .await?;
                summary.exported_entities += files;
                summary.exported_files += files;
            } else if root.entity_type.is_document_like() && root.content_block_id.is_some() {
                let path = self.flat_file_path_for(&root).await?;
                self.export_one_file(
                    &root,
                    path,
                    None,
                    None,
                    request.include_frontmatter_identity,
                    &mut sidecar_metadata,
                )
                .await?;
                summary.exported_entities += 1;
                summary.exported_files += 1;
            } else {
                summary.skipped_entities += 1;
            }
        }

        self.write_sidecar_snapshot_with_metadata(&sidecar_metadata)
            .await?;
        Ok(summary)
    }

    async fn filter_roots(&self, entities: Vec<StoredEntity>) -> anyhow::Result<Vec<StoredEntity>> {
        let relation = RelationType::structure_contained_in();
        let mut out = Vec::new();
        for entity in entities {
            let parents = self
                .stores
                .entity()
                .get_relations_from(&entity.id, Some(&relation))
                .await?;
            if parents.is_empty() {
                out.push(entity);
            }
        }
        Ok(out)
    }

    fn export_tab_tree<'a>(
        &'a self,
        entity: &'a StoredEntity,
        relative_dir: &'a str,
        parent_entity_id: Option<EntityId>,
        position: Option<i64>,
        include_frontmatter_identity: bool,
        sidecar_metadata: &'a mut HashMap<String, SidecarMetadata>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<usize>> + Send + 'a>> {
        Box::pin(async move {
            let children = self
                .coordinator
                .list_children(&entity.id, &RelationType::structure_contained_in())
                .await?;

            let (file_path, child_dir) = if parent_entity_id.is_none() {
                (format!("{relative_dir}/index.md"), relative_dir.to_string())
            } else if children.is_empty() {
                (
                    format!(
                        "{relative_dir}/{}.md",
                        numbered_path_segment(position, &entity_title(entity))
                    ),
                    relative_dir.to_string(),
                )
            } else {
                let dir = format!(
                    "{relative_dir}/{}",
                    numbered_path_segment(position, &entity_title(entity))
                );
                (format!("{dir}/index.md"), dir)
            };

            self.export_one_file(
                entity,
                file_path,
                parent_entity_id.clone(),
                position,
                include_frontmatter_identity,
                sidecar_metadata,
            )
            .await?;

            let mut count = 1;
            for (child, child_position) in children {
                count += self
                    .export_tab_tree(
                        &child,
                        &child_dir,
                        Some(entity.id.clone()),
                        child_position,
                        include_frontmatter_identity,
                        sidecar_metadata,
                    )
                    .await?;
            }
            Ok(count)
        })
    }

    async fn export_one_file(
        &self,
        entity: &StoredEntity,
        relative_path: String,
        parent_entity_id: Option<EntityId>,
        position: Option<i64>,
        include_frontmatter_identity: bool,
        sidecar_metadata: &mut HashMap<String, SidecarMetadata>,
    ) -> anyhow::Result<WrittenMarkdownFile> {
        let body = self
            .coordinator
            .resolve_entity_text(&entity.id)
            .await?
            .unwrap_or_default();
        let relative_path = self.available_path(&entity.id, &relative_path).await?;
        let written = if include_frontmatter_identity {
            let frontmatter = Frontmatter {
                title: entity.name.clone(),
                ..Frontmatter::default()
            };
            let system = SystemFrontmatter {
                id: entity.id.clone(),
                kind: entity.entity_type.clone(),
                origin: entity.origin.clone(),
                privacy: Some(
                    if entity.is_private {
                        "private"
                    } else {
                        "public"
                    }
                    .to_string(),
                ),
            };
            write_frontmatter_markdown(&self.root, &relative_path, &frontmatter, &system, &body)?
        } else {
            write_plain_markdown(&self.root, &relative_path, &body)?
        };

        self.upsert_vault_file(entity.id.clone(), &written).await?;
        let parent_relation = parent_entity_id
            .as_ref()
            .map(|_| RelationType::structure_contained_in());
        sidecar_metadata.insert(
            relative_path,
            SidecarMetadata {
                kind: entity.entity_type.clone(),
                title: entity.name.clone(),
                origin: entity.origin.clone(),
                parent_entity_id,
                parent_relation,
                position,
            },
        );
        Ok(written)
    }

    async fn upsert_vault_file(
        &self,
        entity_id: EntityId,
        written: &WrittenMarkdownFile,
    ) -> anyhow::Result<()> {
        self.stores
            .vault()
            .upsert_vault_file(&VaultFile {
                entity_id,
                path: written.relative_path.clone(),
                file_key: None,
                mtime: written.mtime,
                content_hash: written.content_hash.clone(),
                frontmatter_hash: written.frontmatter_hash.clone(),
                sync_status: VaultSyncStatus::synced(),
                last_seen_at: unix_timestamp(),
            })
            .await
    }

    async fn root_dir_for(&self, entity: &StoredEntity) -> anyhow::Result<String> {
        if let Some(file) = self.stores.vault().get_vault_file(&entity.id).await? {
            if let Some((dir, "index.md")) = file.path.rsplit_once('/') {
                return Ok(dir.to_string());
            }
        }
        let candidate = sanitize_path_segment(&entity_title(entity));
        self.available_dir(&entity.id, &candidate).await
    }

    async fn flat_file_path_for(&self, entity: &StoredEntity) -> anyhow::Result<String> {
        if let Some(file) = self.stores.vault().get_vault_file(&entity.id).await? {
            return Ok(file.path);
        }
        let candidate = format!("{}.md", sanitize_path_segment(&entity_title(entity)));
        self.available_path(&entity.id, &candidate).await
    }

    async fn available_dir(&self, entity_id: &EntityId, candidate: &str) -> anyhow::Result<String> {
        let mut suffix = 1;
        loop {
            let dir = if suffix == 1 {
                candidate.to_string()
            } else {
                format!("{candidate} {suffix}")
            };
            let index_path = format!("{dir}/index.md");
            if self.path_is_available(entity_id, &index_path).await? {
                return Ok(dir);
            }
            suffix += 1;
        }
    }

    async fn available_path(
        &self,
        entity_id: &EntityId,
        candidate: &str,
    ) -> anyhow::Result<String> {
        let candidate = candidate.trim_start_matches('/').to_string();
        let (stem, ext) = match candidate.rsplit_once('.') {
            Some((stem, ext)) => (stem.to_string(), format!(".{ext}")),
            None => (candidate.clone(), String::new()),
        };

        let mut suffix = 1;
        loop {
            let path = if suffix == 1 {
                candidate.clone()
            } else {
                format!("{stem} {suffix}{ext}")
            };
            if self.path_is_available(entity_id, &path).await? {
                return Ok(path);
            }
            suffix += 1;
        }
    }

    async fn path_is_available(&self, entity_id: &EntityId, path: &str) -> anyhow::Result<bool> {
        Ok(
            match self.stores.vault().get_vault_file_by_path(path).await? {
                Some(existing) => existing.entity_id == *entity_id,
                None => true,
            },
        )
    }

    async fn write_sidecar_snapshot_with_metadata(
        &self,
        metadata_by_path: &HashMap<String, SidecarMetadata>,
    ) -> anyhow::Result<()> {
        let files = self.stores.vault().list_vault_files(None).await?;
        let mut manifest = VaultSidecarManifest::from_vault_files(&files);
        if let Some(existing) = read_sidecar_manifest(&self.root)? {
            manifest.preserve_user_fields_from(&existing);
        }
        for (path, metadata) in metadata_by_path {
            if let Some(entry) = manifest.files.get_mut(path) {
                apply_sidecar_metadata(entry, metadata);
            }
        }
        write_sidecar_manifest(&self.root, &manifest)
    }

    async fn scan_and_project(&self) -> anyhow::Result<VaultScanSummary> {
        let reconciler = VaultReconciler::new(self.root.clone(), self.stores.vault());
        let summary = reconciler.reconcile_full_scan().await?;
        let (content_snapshots, asset_projections) = self.project_synced_files().await?;
        Ok(to_scan_summary(
            summary,
            content_snapshots,
            asset_projections,
        ))
    }

    async fn scan_paths_and_project(&self, paths: &[PathBuf]) -> anyhow::Result<VaultScanSummary> {
        let reconciler = VaultReconciler::new(self.root.clone(), self.stores.vault());
        let summary = reconciler.reconcile_paths(paths).await?;
        let (content_snapshots, asset_projections) = self.project_synced_files().await?;
        Ok(to_scan_summary(
            summary,
            content_snapshots,
            asset_projections,
        ))
    }

    fn vault_snapshot(&self) -> anyhow::Result<HashMap<PathBuf, VaultFileFingerprint>> {
        if !self.root.exists() {
            return Ok(HashMap::new());
        }

        let mut snapshot = HashMap::new();
        for file in scan_vault(&self.root, &VaultScanOptions::default())? {
            snapshot.insert(
                PathBuf::from(file.path),
                VaultFileFingerprint {
                    mtime: file.mtime,
                    content_hash: file.content_hash,
                    frontmatter_hash: file.frontmatter_hash,
                },
            );
        }
        Ok(snapshot)
    }

    async fn project_synced_files(&self) -> anyhow::Result<(usize, usize)> {
        let files = self
            .stores
            .vault()
            .list_vault_files(Some(&VaultSyncStatus::synced()))
            .await?;
        let mut content_snapshots = 0;
        let mut asset_projections = 0;

        for file in files {
            let Some(text) = read_markdown_text(&self.root, &file.path)? else {
                continue;
            };
            let (frontmatter, body) = match parse_markdown(&text) {
                Ok(document) => (document.frontmatter, document.body),
                Err(e) => {
                    tracing::warn!(
                        path = %file.path,
                        error = %e,
                        "vault frontmatter metadata projection skipped"
                    );
                    (None, split_markdown(&text).body.to_string())
                }
            };
            let Some(entity) = self.stores.entity().get_entity(&file.entity_id).await? else {
                continue;
            };
            self.project_frontmatter_metadata(&entity, frontmatter.as_ref())
                .await?;
            let current = match entity.content_block_id.as_ref() {
                Some(block_id) => self.stores.text().get_text(block_id).await?,
                None => None,
            };
            if current.as_deref().map(content_hash) != Some(content_hash(&body)) {
                let origin = content_origin_for_entity(&entity);
                self.coordinator
                    .update_entity_content(&entity.id, &body, origin)
                    .await?;
                self.enqueue_embedding(&entity.id, &entity.entity_type, &body)
                    .await;
                content_snapshots += 1;
            }

            let asset_ids = self.asset_ids_from_markdown(&body).await?;
            self.coordinator
                .set_entity_assets(&entity.id, &asset_ids)
                .await?;
            asset_projections += 1;
        }

        Ok((content_snapshots, asset_projections))
    }

    async fn project_frontmatter_metadata(
        &self,
        entity: &StoredEntity,
        frontmatter: Option<&Frontmatter>,
    ) -> anyhow::Result<()> {
        let Some(frontmatter) = frontmatter else {
            return Ok(());
        };

        let mut updated = entity_model(entity);
        let mut changed = false;

        if let Some(title) = frontmatter.title.as_ref().map(|title| title.trim()) {
            if !title.is_empty() && entity.name.as_deref() != Some(title) {
                updated.name = Some(title.to_string());
                changed = true;
            }
        }

        if let Some(tags) = frontmatter.tags.as_ref() {
            let tags_value = Value::Array(tags.iter().cloned().map(Value::String).collect());
            let mut metadata = match updated.metadata.take() {
                Some(Value::Object(map)) => map,
                None => Map::new(),
                Some(other) => {
                    updated.metadata = Some(other);
                    if changed {
                        self.stores
                            .entity()
                            .update_entity(&entity.id, &updated)
                            .await?;
                    }
                    return Ok(());
                }
            };
            if metadata.get("tags") != Some(&tags_value) {
                metadata.insert("tags".to_string(), tags_value);
                updated.metadata = Some(Value::Object(metadata));
                changed = true;
            } else {
                updated.metadata = Some(Value::Object(metadata));
            }
        }

        if changed {
            self.stores
                .entity()
                .update_entity(&entity.id, &updated)
                .await?;
        }
        Ok(())
    }

    async fn asset_ids_from_markdown(&self, markdown: &str) -> anyhow::Result<Vec<CoreAssetId>> {
        let hashes = extract_api_blob_hashes(markdown);
        let mut ids = Vec::new();
        for hash in hashes {
            let blob_hash = BlobHash::from_string(hash);
            if let Some(asset) = self.stores.asset().get_by_blob_hash(&blob_hash).await? {
                ids.push(asset.id);
            }
        }
        Ok(ids)
    }

    async fn enqueue_embedding(&self, entity_id: &EntityId, kind: &EntityType, text: &str) {
        let Some(queue) = self.embedding_queue.as_ref() else {
            return;
        };
        let Some(content_block_id) = self
            .stores
            .entity()
            .get_entity(entity_id)
            .await
            .ok()
            .flatten()
            .and_then(|e| e.content_block_id.clone())
        else {
            return;
        };

        queue
            .enqueue(crate::services::embedding_queue::EmbedJob {
                content_block_id,
                entity_id: entity_id.clone(),
                entity_kind: kind.as_str().to_string(),
                frontmatter: None,
                text: text.to_string(),
            })
            .await;
    }

    async fn conflict_by_id(
        &self,
        conflict_id: &str,
    ) -> anyhow::Result<simply_core::storage::types::VaultConflict> {
        let id = VaultConflictId::from_string(conflict_id);
        self.stores
            .vault()
            .list_vault_conflicts()
            .await?
            .into_iter()
            .find(|conflict| conflict.id == id)
            .ok_or_else(|| anyhow::anyhow!("vault conflict not found: {conflict_id}"))
    }

    async fn resolve(
        &self,
        user_id: &UserId,
        request: ResolveVaultConflictRequest,
    ) -> anyhow::Result<()> {
        let conflict = self.conflict_by_id(&request.conflict_id).await?;
        match request.action {
            VaultConflictResolutionAction::Ignore => {
                self.ignore_conflict_path(&conflict.path)?;
                if let Some(entity_id) = conflict.entity_id.as_ref() {
                    self.stores.vault().delete_vault_file(entity_id).await?;
                }
                self.stores
                    .vault()
                    .clear_vault_conflicts_for_path(&conflict.path)
                    .await?;
            }
            VaultConflictResolutionAction::BindToEntity => {
                let entity_id = request
                    .entity_id
                    .as_deref()
                    .map(EntityId::from_string)
                    .or(conflict.entity_id.clone())
                    .ok_or_else(|| anyhow::anyhow!("bind requires an entity id"))?;
                self.verify_access(user_id, &entity_id).await?;
                let (_, written) = observed_markdown_file(&self.root, &conflict.path)?;
                self.upsert_vault_file(entity_id, &written).await?;
                self.stores
                    .vault()
                    .delete_vault_conflict(&conflict.id)
                    .await?;
            }
            VaultConflictResolutionAction::AcceptNewPath => {
                let entity_id = conflict
                    .entity_id
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("accept_new_path requires canonical entity"))?;
                self.verify_access(user_id, &entity_id).await?;
                let (_, written) = observed_markdown_file(&self.root, &conflict.path)?;
                self.upsert_vault_file(entity_id, &written).await?;
                self.stores
                    .vault()
                    .delete_vault_conflict(&conflict.id)
                    .await?;
            }
            VaultConflictResolutionAction::ForkAsNewDocument => {
                let (body, written) = observed_markdown_file(&self.root, &conflict.path)?;
                let title = Path::new(&conflict.path)
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("Forked document");
                let entity_id = self
                    .coordinator
                    .create_entity_with_content(
                        EntityType::new("document::note"),
                        Some(user_id),
                        Some(title),
                        Some((&body, ContentOrigin::user(user_id.clone()))),
                        None,
                    )
                    .await?;
                self.upsert_vault_file(entity_id, &written).await?;
                self.stores
                    .vault()
                    .delete_vault_conflict(&conflict.id)
                    .await?;
            }
            VaultConflictResolutionAction::RestoreOriginalId => {
                let entity_id = conflict.entity_id.clone().ok_or_else(|| {
                    anyhow::anyhow!("restore_original_id requires canonical entity")
                })?;
                let entity = self.verify_access(user_id, &entity_id).await?;
                let body = read_markdown_body(&self.root, &conflict.path)?.unwrap_or_default();
                let frontmatter = Frontmatter {
                    title: entity.name.clone(),
                    ..Frontmatter::default()
                };
                let system = SystemFrontmatter {
                    id: entity.id.clone(),
                    kind: entity.entity_type.clone(),
                    origin: entity.origin.clone(),
                    privacy: Some(
                        if entity.is_private {
                            "private"
                        } else {
                            "public"
                        }
                        .to_string(),
                    ),
                };
                let written = write_frontmatter_markdown(
                    &self.root,
                    &conflict.path,
                    &frontmatter,
                    &system,
                    &body,
                )?;
                self.upsert_vault_file(entity.id.clone(), &written).await?;
                self.stores
                    .vault()
                    .delete_vault_conflict(&conflict.id)
                    .await?;
            }
        }
        self.project_synced_files().await?;
        self.write_sidecar_snapshot_with_metadata(&HashMap::new())
            .await
    }

    fn ignore_conflict_path(&self, path: &str) -> anyhow::Result<()> {
        let mut manifest = read_sidecar_manifest(&self.root)?.unwrap_or_default();
        manifest.ignored_paths.insert(path.to_string());
        write_sidecar_manifest(&self.root, &manifest)
    }
}

#[async_trait]
impl<S: StorageTypes> VaultApi for VaultService<S> {
    async fn export_documents(
        &self,
        ctx: &RequestContext,
        request: VaultExportRequest,
    ) -> anyhow::Result<VaultExportSummary> {
        let user_id = Self::require_user(ctx)?;
        self.export_roots(&user_id, request).await
    }

    async fn scan(&self, ctx: &RequestContext) -> anyhow::Result<VaultScanSummary> {
        let _ = Self::require_user(ctx)?;
        self.scan_and_project().await
    }

    async fn list_conflicts(&self, ctx: &RequestContext) -> anyhow::Result<Vec<VaultConflictInfo>> {
        let user_id = Self::require_user(ctx)?;
        let mut out = Vec::new();
        for conflict in self.stores.vault().list_vault_conflicts().await? {
            if let Some(entity_id) = &conflict.entity_id {
                self.verify_access(&user_id, entity_id).await?;
            }
            out.push(VaultConflictInfo {
                id: conflict.id.to_string(),
                entity_id: conflict.entity_id.map(|id| id.to_string()),
                path: conflict.path,
                reason: conflict.reason.as_str().to_string(),
                observed_entity_id: conflict.observed_entity_id.map(|id| id.to_string()),
                details: conflict.details,
                created_at: conflict.created_at,
            });
        }
        Ok(out)
    }

    async fn resolve_conflict(
        &self,
        ctx: &RequestContext,
        request: ResolveVaultConflictRequest,
    ) -> anyhow::Result<()> {
        let user_id = Self::require_user(ctx)?;
        self.resolve(&user_id, request).await
    }
}

fn apply_sidecar_metadata(entry: &mut VaultSidecarFile, metadata: &SidecarMetadata) {
    entry.kind = Some(metadata.kind.clone());
    entry.title = metadata.title.clone();
    entry.origin = metadata.origin.clone();
    entry.parent_entity_id = metadata.parent_entity_id.clone();
    entry.parent_relation = metadata.parent_relation.clone();
    entry.position = metadata.position;
}

fn changed_snapshot_paths(
    previous: &HashMap<PathBuf, VaultFileFingerprint>,
    next: &HashMap<PathBuf, VaultFileFingerprint>,
) -> Vec<PathBuf> {
    let mut paths = HashSet::new();
    for (path, fingerprint) in next {
        if previous.get(path) != Some(fingerprint) {
            paths.insert(path.clone());
        }
    }
    for path in previous.keys() {
        if !next.contains_key(path) {
            paths.insert(path.clone());
        }
    }

    let mut paths: Vec<_> = paths.into_iter().collect();
    paths.sort();
    paths
}

fn observed_markdown_file(
    root: &Path,
    relative_path: &str,
) -> anyhow::Result<(String, WrittenMarkdownFile)> {
    let Some(text) = read_markdown_text(root, relative_path)? else {
        anyhow::bail!("vault file not found: {relative_path}");
    };
    let split = split_markdown(&text);
    let path = root.join(relative_path);
    let mtime = std::fs::metadata(&path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| {
            modified
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_millis() as i64)
        });

    Ok((
        split.body.to_string(),
        WrittenMarkdownFile {
            relative_path: relative_path.to_string(),
            mtime,
            content_hash: content_hash(split.body),
            frontmatter_hash: split.raw_frontmatter.map(content_hash),
        },
    ))
}

fn entity_model(entity: &StoredEntity) -> Entity {
    Entity {
        entity_type: entity.entity_type.clone(),
        user_id: entity.user_id.clone(),
        name: entity.name.clone(),
        is_private: entity.is_private,
        content_block_id: entity.content_block_id.clone(),
        origin: entity.origin.clone(),
        metadata: entity.metadata.clone(),
    }
}

fn entity_title(entity: &StoredEntity) -> String {
    entity
        .name
        .clone()
        .unwrap_or_else(|| entity.id.as_str().to_string())
}

fn content_origin_for_entity(entity: &StoredEntity) -> ContentOrigin {
    if let Some(origin) = entity
        .origin
        .as_deref()
        .and_then(|origin| origin.split_once(':'))
        .map(|(_, id)| ContentOrigin::import(id.to_string()))
    {
        return match entity.user_id.as_ref() {
            Some(user_id) => origin.with_user(user_id.clone()),
            None => origin,
        };
    }

    match entity.user_id.as_ref() {
        Some(user_id) => ContentOrigin::user(user_id.clone()),
        None => ContentOrigin::system(),
    }
}

fn to_scan_summary(
    summary: VaultReconciliationSummary,
    content_snapshots: usize,
    asset_projections: usize,
) -> VaultScanSummary {
    VaultScanSummary {
        scanned_files: summary.scanned_files,
        actions: summary.actions,
        projected_files: summary.projected_files,
        conflicts: summary.conflicts,
        missing_files: summary.missing_files,
        unmanaged_files: summary.unmanaged_files,
        content_snapshots,
        asset_projections,
    }
}

fn extract_api_blob_hashes(markdown: &str) -> Vec<String> {
    let mut hashes = HashSet::new();
    for part in markdown.split("/api/blob/").skip(1) {
        let hash: String = part.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        if !hash.is_empty() {
            hashes.insert(hash);
        }
    }
    hashes.into_iter().collect()
}
