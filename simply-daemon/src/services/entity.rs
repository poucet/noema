//! Entity CRUD service — generic entity/relation/content surface over the
//! `StorageCoordinator` primitives. Implements `EntityApi`.
//!
//! Writes route through the coordinator so orphan cleanup (relations,
//! entity_assets mappings, content-block replacement) happens consistently.
//! Reads hit `entity_store` directly where possible.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;

use simply_core::storage::coordinator::StorageCoordinator;
use simply_core::storage::ids::{AssetId as CoreAssetId, EntityId};
use simply_core::storage::traits::{EntityStore, StorageTypes, Stores, StoredEntity};
use simply_core::storage::types::entity::origin_scheme;
use simply_core::storage::types::{ContentOrigin, EntityType, RelationType};
use simply_rpc::RequestContext;

use crate::api::*;
use crate::types::AssetId;

/// Relations whose incoming/outgoing edge-counts are useful on an
/// `EntitySummary`. We query these for every summary; more exotic relations
/// are inspected on demand.
const SUMMARY_COUNT_RELATIONS: &[&str] = &[
    "structure::contained_in",
    "reference::to",
];

/// Kinds callers of the public API are allowed to create or convert entities
/// into. Restricted to the `document::` namespace so callers can't
/// accidentally mint `conversation` entities (daemon-owned), `system::*`
/// organizational primitives, or other future non-document kinds.
/// Import skills still reach any kind through the same API (they create
/// `document::tabbed` + `document::tab`), which is fine.
fn is_user_creatable_kind(kind: &str) -> bool {
    kind.starts_with("document::")
}

pub struct EntityService<S: StorageTypes> {
    coordinator: Arc<StorageCoordinator<S>>,
    stores: Arc<dyn Stores<S>>,
    embedding_queue: Option<Arc<dyn crate::services::embedding_queue::EmbeddingQueue>>,
    user_email_cache: Option<Arc<crate::services::user::UserEmailCache>>,
}

impl<S: StorageTypes> EntityService<S> {
    pub fn new(
        coordinator: Arc<StorageCoordinator<S>>,
        stores: Arc<dyn Stores<S>>,
    ) -> Self {
        Self {
            coordinator,
            stores,
            embedding_queue: None,
            user_email_cache: None,
        }
    }

    pub fn with_embedding(
        mut self,
        queue: Arc<dyn crate::services::embedding_queue::EmbeddingQueue>,
    ) -> Self {
        self.embedding_queue = Some(queue);
        self
    }

    pub fn with_user_email_cache(
        mut self,
        cache: Arc<crate::services::user::UserEmailCache>,
    ) -> Self {
        self.user_email_cache = Some(cache);
        self
    }

    fn require_user(ctx: &RequestContext) -> anyhow::Result<simply_core::storage::ids::UserId> {
        ctx.scope.user_id.as_ref()
            .map(|id| simply_core::storage::ids::UserId::from_string(id))
            .ok_or_else(|| anyhow::anyhow!("authentication required"))
    }

    /// Verify a user can access an entity (owner or the entity has no owner).
    async fn verify_access(
        &self,
        user_id: &simply_core::storage::ids::UserId,
        entity_id: &EntityId,
    ) -> anyhow::Result<StoredEntity> {
        let entity = self.stores.entity().get_entity(entity_id).await?
            .ok_or_else(|| anyhow::anyhow!("entity not found: {entity_id}"))?;
        match entity.user_id.as_ref() {
            Some(uid) if uid == user_id => Ok(entity),
            None => Ok(entity),
            Some(_) => anyhow::bail!("access denied: entity belongs to another user"),
        }
    }

    /// Convert a `StoredEntity` to a wire `EntitySummary`, populating
    /// `has_content`, `owner_email`, and per-relation incoming/outgoing edge
    /// counts for the relations we surface by default.
    async fn to_summary(&self, entity: StoredEntity) -> anyhow::Result<EntitySummary> {
        let owner_email = match (&entity.user_id, &self.user_email_cache) {
            (Some(uid), Some(cache)) => cache.get(&self.stores, uid).await,
            _ => None,
        };

        // Query incoming + outgoing counts for the well-known relations. Each
        // direction is a round-trip; for bulk listings the caller should page
        // or specialize. Zero-count relations are omitted so clients can
        // cheaply check existence via `.has(rel)`.
        let mut incoming_counts: BTreeMap<String, u32> = BTreeMap::new();
        let mut outgoing_counts: BTreeMap<String, u32> = BTreeMap::new();
        for rel_name in SUMMARY_COUNT_RELATIONS {
            let rel = RelationType::new(*rel_name);
            let incoming = self.stores.entity().get_relations_to(&entity.id, Some(&rel)).await?;
            if !incoming.is_empty() {
                incoming_counts.insert((*rel_name).to_string(), incoming.len() as u32);
            }
            let outgoing = self.stores.entity().get_relations_from(&entity.id, Some(&rel)).await?;
            if !outgoing.is_empty() {
                outgoing_counts.insert((*rel_name).to_string(), outgoing.len() as u32);
            }
        }

        Ok(EntitySummary {
            id: entity.id.to_string(),
            kind: entity.entity_type.as_str().to_string(),
            title: entity.name.clone(),
            origin: entity.origin.clone(),
            user_id: entity.user_id.as_ref().map(|u| u.to_string()),
            owner_email,
            is_private: entity.is_private,
            created_at: entity.content.created_at,
            updated_at: entity.content.updated_at,
            has_content: entity.content_block_id.is_some(),
            incoming_counts,
            outgoing_counts,
        })
    }

    async fn entities_to_summaries(&self, entities: Vec<StoredEntity>) -> anyhow::Result<Vec<EntitySummary>> {
        let mut out = Vec::with_capacity(entities.len());
        for e in entities {
            out.push(self.to_summary(e).await?);
        }
        Ok(out)
    }

    /// If `relation` is set, drop any entity that has an outgoing edge of
    /// that relation — i.e. keep only the roots of the relation's forest.
    /// If `relation` is `None`, pass everything through unchanged.
    async fn filter_roots(
        &self,
        entities: Vec<StoredEntity>,
        relation: Option<&str>,
    ) -> anyhow::Result<Vec<StoredEntity>> {
        let Some(rel_name) = relation else { return Ok(entities); };
        let rel = RelationType::new(rel_name);
        let mut out = Vec::with_capacity(entities.len());
        for e in entities {
            let outgoing = self
                .stores
                .entity()
                .get_relations_from(&e.id, Some(&rel))
                .await?;
            if outgoing.is_empty() {
                out.push(e);
            }
        }
        Ok(out)
    }

    /// Enqueue a content-bearing entity for embedding. The owning document
    /// plumbing from the legacy DocumentService goes through tab/doc ids; the
    /// entity world will be wired up by the RAG pivot (Commit 8). For now
    /// embedding triggered from this service is a no-op until that work lands.
    fn enqueue_embedding(
        &self,
        _entity_id: &EntityId,
        _kind: &EntityType,
        _user_id: &simply_core::storage::ids::UserId,
        _text: &str,
    ) {
        // Intentionally empty until Commit 8 pivots EmbedJob onto content_block_id.
    }
}

#[async_trait]
impl<S: StorageTypes> EntityApi for EntityService<S> {
    // ----- Listing ---------------------------------------------------------

    async fn list_entities(
        &self,
        ctx: &RequestContext,
        type_prefix: Option<String>,
        root_of_relation: Option<String>,
    ) -> anyhow::Result<Vec<EntitySummary>> {
        let user_id = Self::require_user(ctx)?;
        let entities = match type_prefix.as_deref() {
            Some(prefix) if !prefix.is_empty() => {
                self.stores.entity().list_entities_by_type_prefix(&user_id, prefix).await?
            }
            _ => self.stores.entity().list_entities(&user_id, None).await?,
        };
        let roots = self.filter_roots(entities, root_of_relation.as_deref()).await?;
        self.entities_to_summaries(roots).await
    }

    async fn search_entities(
        &self,
        ctx: &RequestContext,
        query: &str,
        type_prefix: Option<String>,
        root_of_relation: Option<String>,
    ) -> anyhow::Result<Vec<EntitySummary>> {
        let user_id = Self::require_user(ctx)?;
        let all = match type_prefix.as_deref() {
            Some(prefix) if !prefix.is_empty() => {
                self.stores.entity().list_entities_by_type_prefix(&user_id, prefix).await?
            }
            _ => self.stores.entity().list_entities(&user_id, None).await?,
        };
        let needle = query.to_lowercase();
        let matching: Vec<_> = all
            .into_iter()
            .filter(|e| e.name.as_deref().map_or(false, |n| n.to_lowercase().contains(&needle)))
            .take(50)
            .collect();
        let roots = self.filter_roots(matching, root_of_relation.as_deref()).await?;
        self.entities_to_summaries(roots).await
    }

    // ----- Entity CRUD -----------------------------------------------------

    async fn get_entity(
        &self,
        ctx: &RequestContext,
        entity_id: &str,
    ) -> anyhow::Result<EntitySummary> {
        let user_id = Self::require_user(ctx)?;
        let id = EntityId::from_string(entity_id);
        let entity = self.verify_access(&user_id, &id).await?;
        self.to_summary(entity).await
    }

    async fn get_entity_by_origin(
        &self,
        ctx: &RequestContext,
        origin: &str,
    ) -> anyhow::Result<Option<EntitySummary>> {
        let user_id = Self::require_user(ctx)?;
        let entity = self
            .stores
            .entity()
            .get_entity_by_origin(&user_id, origin)
            .await?;
        match entity {
            Some(e) => Ok(Some(self.to_summary(e).await?)),
            None => Ok(None),
        }
    }

    async fn create_entity(
        &self,
        ctx: &RequestContext,
        request: CreateEntityRequest,
    ) -> anyhow::Result<EntitySummary> {
        let user_id = Self::require_user(ctx)?;
        if !is_user_creatable_kind(&request.kind) {
            anyhow::bail!(
                "entity kind `{}` is not user-creatable (must start with `document::`)",
                request.kind
            );
        }
        let kind = EntityType::new(request.kind);

        // Build a ContentOrigin for the new content block (if any).
        let content_origin = match request.origin.as_deref().and_then(|o| {
            let (scheme, id) = o.split_once(':')?;
            Some((scheme.to_string(), id.to_string()))
        }) {
            Some((scheme, ext_id)) if scheme == origin_scheme::GOOGLE_DRIVE => {
                ContentOrigin::import(ext_id).with_user(user_id.clone())
            }
            _ => ContentOrigin::user(user_id.clone()),
        };

        let content = request.content.as_deref().map(|text| (text, content_origin));

        let entity_id = self
            .coordinator
            .create_entity_with_content(
                kind.clone(),
                Some(&user_id),
                request.title.as_deref(),
                content,
                request.origin.as_deref(),
            )
            .await?;

        // Track referenced assets for blob GC.
        if !request.referenced_assets.is_empty() {
            let core_assets: Vec<CoreAssetId> = request
                .referenced_assets
                .iter()
                .map(|a| CoreAssetId::from_string(a.as_str()))
                .collect();
            self.coordinator.set_entity_assets(&entity_id, &core_assets).await?;
        }

        // Kick off embedding for content-bearing entities (no-op until Commit 8).
        if let Some(text) = request.content.as_deref() {
            self.enqueue_embedding(&entity_id, &kind, &user_id, text);
        }

        let entity = self.stores.entity().get_entity(&entity_id).await?
            .ok_or_else(|| anyhow::anyhow!("entity disappeared after create: {entity_id}"))?;
        self.to_summary(entity).await
    }

    async fn rename_entity(
        &self,
        ctx: &RequestContext,
        entity_id: &str,
        name: &str,
    ) -> anyhow::Result<()> {
        let user_id = Self::require_user(ctx)?;
        let id = EntityId::from_string(entity_id);
        let mut entity = self.verify_access(&user_id, &id).await?;
        entity.name = Some(name.to_string());
        self.stores.entity().update_entity(&id, &entity).await
    }

    async fn change_entity_kind(
        &self,
        ctx: &RequestContext,
        entity_id: &str,
        new_kind: &str,
    ) -> anyhow::Result<()> {
        let user_id = Self::require_user(ctx)?;
        if !is_user_creatable_kind(new_kind) {
            anyhow::bail!(
                "entity kind `{}` cannot be set from the public API (must start with `document::`)",
                new_kind
            );
        }
        let id = EntityId::from_string(entity_id);
        self.verify_access(&user_id, &id).await?;
        self.stores
            .entity()
            .set_entity_type(&id, EntityType::new(new_kind))
            .await
    }

    async fn delete_entity(
        &self,
        ctx: &RequestContext,
        entity_id: &str,
    ) -> anyhow::Result<()> {
        let user_id = Self::require_user(ctx)?;
        let id = EntityId::from_string(entity_id);
        let _ = self.verify_access(&user_id, &id).await?;

        // Walk structure::contained_in to remove the whole tree (e.g. a
        // tabbed doc with its tabs). Coordinator handles per-entity relation
        // and asset-mapping cleanup.
        self.coordinator
            .delete_entity_cascade(&id, &[RelationType::structure_contained_in()])
            .await
    }

    // ----- Content --------------------------------------------------------

    async fn get_entity_content(
        &self,
        ctx: &RequestContext,
        entity_id: &str,
    ) -> anyhow::Result<EntityContent> {
        let user_id = Self::require_user(ctx)?;
        let id = EntityId::from_string(entity_id);
        self.verify_access(&user_id, &id).await?;

        let content_markdown = self.coordinator.resolve_entity_text(&id).await?;
        let assets = self.coordinator.get_entity_assets(&id).await?;
        let referenced_assets: Vec<AssetId> = assets
            .into_iter()
            .map(|a| AssetId::from(a.as_str().to_string()))
            .collect();

        Ok(EntityContent {
            entity_id: entity_id.to_string(),
            content_markdown,
            referenced_assets,
        })
    }

    async fn update_entity_content(
        &self,
        ctx: &RequestContext,
        entity_id: &str,
        request: UpdateEntityContentRequest,
    ) -> anyhow::Result<()> {
        let user_id = Self::require_user(ctx)?;
        let id = EntityId::from_string(entity_id);
        let entity = self.verify_access(&user_id, &id).await?;

        // Build a content origin based on the entity's own origin scheme.
        let content_origin = entity
            .origin
            .as_deref()
            .and_then(|o| o.split_once(':'))
            .map(|(scheme, ext_id)| match scheme {
                s if s == origin_scheme::GOOGLE_DRIVE => {
                    ContentOrigin::import(ext_id).with_user(user_id.clone())
                }
                _ => ContentOrigin::user(user_id.clone()),
            })
            .unwrap_or_else(|| ContentOrigin::user(user_id.clone()));

        self.coordinator
            .update_entity_content(&id, &request.content, content_origin)
            .await?;

        // Refresh entity_assets.
        let core_assets: Vec<CoreAssetId> = request
            .referenced_assets
            .iter()
            .map(|a| CoreAssetId::from_string(a.as_str()))
            .collect();
        self.coordinator.set_entity_assets(&id, &core_assets).await?;

        self.enqueue_embedding(&id, &entity.entity_type, &user_id, &request.content);
        Ok(())
    }

    async fn flush_entity_embedding(
        &self,
        _ctx: &RequestContext,
        _entity_id: &str,
    ) -> anyhow::Result<()> {
        // Embedding flush is a no-op on the entity path until Commit 8 rewires
        // EmbedJob onto content_block_id. Callers invoking this today should
        // continue using the legacy DocumentApi flush endpoint for tab-backed
        // data; flat entities don't yet participate in embeddings.
        Ok(())
    }

    // ----- Relations ------------------------------------------------------

    async fn list_incoming(
        &self,
        ctx: &RequestContext,
        entity_id: &str,
        relation: &str,
    ) -> anyhow::Result<Vec<RelatedEntity>> {
        let user_id = Self::require_user(ctx)?;
        let id = EntityId::from_string(entity_id);
        self.verify_access(&user_id, &id).await?;

        let rel = RelationType::new(relation);
        let related = self
            .stores
            .entity()
            .list_relations_to_ordered(&id, &rel)
            .await?;

        let mut out = Vec::with_capacity(related.len());
        for (other_id, edge) in related {
            let Some(other) = self.stores.entity().get_entity(&other_id).await? else {
                continue;
            };
            out.push(RelatedEntity {
                summary: self.to_summary(other).await?,
                position: edge.position,
            });
        }
        Ok(out)
    }

    async fn list_outgoing(
        &self,
        ctx: &RequestContext,
        entity_id: &str,
        relation: &str,
    ) -> anyhow::Result<Vec<RelatedEntity>> {
        let user_id = Self::require_user(ctx)?;
        let id = EntityId::from_string(entity_id);
        self.verify_access(&user_id, &id).await?;

        let rel = RelationType::new(relation);
        let related = self
            .stores
            .entity()
            .list_relations_from_ordered(&id, &rel)
            .await?;

        let mut out = Vec::with_capacity(related.len());
        for (other_id, edge) in related {
            let Some(other) = self.stores.entity().get_entity(&other_id).await? else {
                continue;
            };
            out.push(RelatedEntity {
                summary: self.to_summary(other).await?,
                position: edge.position,
            });
        }
        Ok(out)
    }

    async fn add_relation(
        &self,
        ctx: &RequestContext,
        request: AddRelationRequest,
    ) -> anyhow::Result<()> {
        let user_id = Self::require_user(ctx)?;
        let from = EntityId::from_string(&request.from_id);
        let to = EntityId::from_string(&request.to_id);
        self.verify_access(&user_id, &from).await?;
        self.verify_access(&user_id, &to).await?;

        let rel = RelationType::new(&request.relation);
        self.stores
            .entity()
            .add_relation(&from, &to, rel, request.position, None)
            .await
    }

    async fn remove_relation(
        &self,
        ctx: &RequestContext,
        from_id: &str,
        to_id: &str,
        relation: &str,
    ) -> anyhow::Result<()> {
        let user_id = Self::require_user(ctx)?;
        let from = EntityId::from_string(from_id);
        let to = EntityId::from_string(to_id);
        self.verify_access(&user_id, &from).await?;

        let rel = RelationType::new(relation);
        self.stores.entity().remove_relation(&from, &to, &rel).await
    }

    async fn move_relation(
        &self,
        ctx: &RequestContext,
        request: MoveRelationRequest,
    ) -> anyhow::Result<()> {
        let user_id = Self::require_user(ctx)?;
        let from = EntityId::from_string(&request.from_id);
        let new_to = EntityId::from_string(&request.new_to_id);
        self.verify_access(&user_id, &from).await?;
        self.verify_access(&user_id, &new_to).await?;

        let rel = RelationType::new(&request.relation);
        self.coordinator
            .move_entity(&from, &new_to, request.new_position, &rel)
            .await
    }
}
