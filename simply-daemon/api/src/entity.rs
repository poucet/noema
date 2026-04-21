//! Entity CRUD API — generic entity/relation/content surface that replaces
//! the document-shaped `DocumentApi` incrementally.
//!
//! The daemon exposes this alongside `DocumentApi` during the UCM transition;
//! clients (gdocs skill, admin UI, Noema UI) migrate one at a time and the
//! legacy surface is removed once there are no callers.
//!
//! Relations in UCM are directed edges in a graph — not a tree. Every edge has
//! a `from` endpoint and a `to` endpoint under a named relation. The API
//! reflects that: a caller asks for an entity's *outgoing* edges (where it's
//! the `from`) or its *incoming* edges (where it's the `to`). Some relations
//! are naturally hierarchical by convention (`structure::contained_in` goes
//! child → parent) but the API does not assume it — `reference::to`,
//! `label::tagged_with`, `conversation::forked_from`, etc. are all queried the
//! same way.
//!
//! Structure-only responses (`EntitySummary`, `RelatedEntity`) never ship
//! content bodies. Content is fetched per-entity on demand via
//! [`EntityApi::get_entity_content`] — the UI loads a tab's markdown only
//! when the user opens it.

use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use simply_rpc::RequestContext;
#[cfg(feature = "ts")]
use ts_rs::TS;

use crate::types::AssetId;

// ============================================================================
// Wire types
// ============================================================================

/// Summary of an entity for listings and structure responses. Never carries
/// content bodies — content is fetched separately via [`EntityApi::get_entity_content`].
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
pub struct EntitySummary {
    pub id: String,
    /// Namespaced entity type (e.g. `"document::tabbed"`, `"document::note"`, `"conversation"`).
    pub kind: String,
    pub title: Option<String>,
    /// URI-like origin string `"<scheme>:<id>"` (e.g. `"google_drive:gdoc-abc"`).
    pub origin: Option<String>,
    pub user_id: Option<String>,
    /// Email of the owning user, if available. Populated for admin listings.
    pub owner_email: Option<String>,
    pub is_private: bool,
    pub created_at: i64,
    pub updated_at: i64,
    /// True if `content_block_id` is set. UI dispatches on this to decide
    /// whether to show a markdown editor.
    pub has_content: bool,
    /// For each relation, count of edges where this entity is the `to`
    /// endpoint (edges pointing *at* this entity). Covers e.g. a tabbed doc's
    /// tabs (`structure::contained_in` — each tab points at the doc) or an
    /// entity's backlinks (`reference::to`).
    pub incoming_counts: BTreeMap<String, u32>,
    /// For each relation, count of edges where this entity is the `from`
    /// endpoint (edges originating *at* this entity). Covers e.g. a tab's
    /// parent doc (`structure::contained_in` — outgoing once) or an entity's
    /// outgoing references / labels.
    pub outgoing_counts: BTreeMap<String, u32>,
}

/// An entity on the far side of a relation edge, bundled with the edge's
/// position (if the relation uses ordering).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
pub struct RelatedEntity {
    pub summary: EntitySummary,
    pub position: Option<i64>,
}

/// Resolved content for an entity. Returned by [`EntityApi::get_entity_content`].
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
pub struct EntityContent {
    pub entity_id: String,
    /// Markdown text for the entity's live content block, if any.
    /// `None` if the entity has no `content_block_id` set.
    pub content_markdown: Option<String>,
    /// Assets referenced by the content (for blob GC to keep images alive).
    #[serde(default)]
    pub referenced_assets: Vec<AssetId>,
}

/// Request to create a new entity. `content` is optional — flat kinds (notes,
/// todos, prompts) typically provide content; container kinds (tabbed docs,
/// directories) do not.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
pub struct CreateEntityRequest {
    /// Namespaced entity type (e.g. `"document::note"`).
    pub kind: String,
    pub title: Option<String>,
    pub content: Option<String>,
    /// Optional `"<scheme>:<id>"` origin (e.g. `"google_drive:gdoc-abc"`).
    /// Callers that want to replace an existing import should look up via
    /// `/entity?origin=...` and delete the old entity first; the service does
    /// not dedupe.
    pub origin: Option<String>,
    #[serde(default)]
    pub referenced_assets: Vec<AssetId>,
}

/// Request to update an entity's content.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
pub struct UpdateEntityContentRequest {
    pub content: String,
    #[serde(default)]
    pub referenced_assets: Vec<AssetId>,
}

/// Request to add a relation edge `from_id → to_id` under a named relation.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
pub struct AddRelationRequest {
    pub from_id: String,
    pub to_id: String,
    /// Namespaced relation (e.g. `"structure::contained_in"`, `"reference::to"`).
    pub relation: String,
    pub position: Option<i64>,
}

/// Request to rewrite an existing edge's `to` endpoint and/or position.
/// Atomically updates the edge `(from_id, relation, *) → (from_id, relation, new_to_id)`
/// and resequences siblings sharing the new endpoint. For `structure::contained_in`
/// this is how drag-and-drop reparenting works; for `reference::to` it re-targets
/// a citation.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "ts/src/generated/types/"))]
pub struct MoveRelationRequest {
    pub from_id: String,
    pub relation: String,
    pub new_to_id: String,
    pub new_position: i64,
}

// ============================================================================
// EntityApi trait
// ============================================================================

#[simply_rpc::rpc_service("entity")]
#[async_trait]
pub trait EntityApi: Send + Sync {
    // ----- Listing -----

    /// List all entities for the authenticated user. Filter by `type_prefix`
    /// for `LIKE 'prefix%'` matching — e.g. `"document::"` for all document
    /// kinds.
    #[rpc(get = "/entity")]
    async fn list_entities(
        &self,
        ctx: &RequestContext,
        type_prefix: Option<String>,
    ) -> anyhow::Result<Vec<EntitySummary>>;

    /// Search entities by title (case-insensitive). Optionally filter by type
    /// prefix.
    #[rpc(get = "/entity/search/{query}")]
    async fn search_entities(
        &self,
        ctx: &RequestContext,
        query: &str,
        type_prefix: Option<String>,
    ) -> anyhow::Result<Vec<EntitySummary>>;

    // ----- Entity CRUD -----

    /// Get an entity's summary (no content).
    #[rpc(get = "/entity/{entity_id}")]
    async fn get_entity(
        &self,
        ctx: &RequestContext,
        entity_id: &str,
    ) -> anyhow::Result<EntitySummary>;

    /// Create a new entity, optionally with initial content.
    #[rpc(post = "/entity", no_tool)]
    async fn create_entity(
        &self,
        ctx: &RequestContext,
        request: CreateEntityRequest,
    ) -> anyhow::Result<EntitySummary>;

    /// Rename an entity.
    #[rpc(put = "/entity/{entity_id}", no_tool)]
    async fn rename_entity(
        &self,
        ctx: &RequestContext,
        entity_id: &str,
        name: &str,
    ) -> anyhow::Result<()>;

    /// Change an entity's kind (e.g. `document::note` → `document::todo`).
    /// Restricted to kinds in the `document::` namespace so UI callers can't
    /// accidentally convert entities into conversations, directories, etc.
    #[rpc(put = "/entity/{entity_id}/kind", no_tool)]
    async fn change_entity_kind(
        &self,
        ctx: &RequestContext,
        entity_id: &str,
        new_kind: &str,
    ) -> anyhow::Result<()>;

    /// Delete an entity and its descendants via `structure::contained_in`.
    #[rpc(delete = "/entity/{entity_id}", no_tool)]
    async fn delete_entity(
        &self,
        ctx: &RequestContext,
        entity_id: &str,
    ) -> anyhow::Result<()>;

    // ----- Content -----

    /// Fetch an entity's live content (lazy). Returns
    /// `content_markdown = None` if the entity has no content block.
    #[rpc(get = "/entity/{entity_id}/content")]
    async fn get_entity_content(
        &self,
        ctx: &RequestContext,
        entity_id: &str,
    ) -> anyhow::Result<EntityContent>;

    /// Replace an entity's content. Creates a new content block; the old one
    /// is orphaned for later GC.
    #[rpc(put = "/entity/{entity_id}/content", no_tool)]
    async fn update_entity_content(
        &self,
        ctx: &RequestContext,
        entity_id: &str,
        request: UpdateEntityContentRequest,
    ) -> anyhow::Result<()>;

    /// Flush pending embedding for an entity (process immediately, bypass
    /// debounce). Called on tab switch / page unload to ensure edits are
    /// embedded promptly.
    #[rpc(post = "/entity/{entity_id}/flush_embedding", no_tool)]
    async fn flush_entity_embedding(
        &self,
        ctx: &RequestContext,
        entity_id: &str,
    ) -> anyhow::Result<()>;

    // ----- Relations -----

    /// List entities with an *incoming* edge at `entity_id` under `relation`
    /// (i.e. edges where `to_id = entity_id`). Ordered by `position` when the
    /// relation uses ordering.
    ///
    /// Examples: a tabbed doc's tabs (`structure::contained_in`), an entity's
    /// backlinks (`reference::to`), the set of notes carrying a label
    /// (`label::tagged_with` on the label).
    #[rpc(get = "/entity/{entity_id}/relations/incoming/{relation}")]
    async fn list_incoming(
        &self,
        ctx: &RequestContext,
        entity_id: &str,
        relation: &str,
    ) -> anyhow::Result<Vec<RelatedEntity>>;

    /// List entities with an *outgoing* edge at `entity_id` under `relation`
    /// (i.e. edges where `from_id = entity_id`). Ordered by `position` when
    /// the relation uses ordering.
    ///
    /// Examples: a tab's parent doc (`structure::contained_in`), an entity's
    /// outgoing citations (`reference::to`), the labels attached to a note
    /// (`label::tagged_with` on the note).
    #[rpc(get = "/entity/{entity_id}/relations/outgoing/{relation}")]
    async fn list_outgoing(
        &self,
        ctx: &RequestContext,
        entity_id: &str,
        relation: &str,
    ) -> anyhow::Result<Vec<RelatedEntity>>;

    /// Add a relation edge `from_id → to_id`.
    #[rpc(post = "/entity/relation", no_tool)]
    async fn add_relation(
        &self,
        ctx: &RequestContext,
        request: AddRelationRequest,
    ) -> anyhow::Result<()>;

    /// Remove a specific edge `(from_id, to_id, relation)`.
    #[rpc(delete = "/entity/relation/{from_id}/{to_id}/{relation}", no_tool)]
    async fn remove_relation(
        &self,
        ctx: &RequestContext,
        from_id: &str,
        to_id: &str,
        relation: &str,
    ) -> anyhow::Result<()>;

    /// Atomically rewrite an edge's `to` endpoint and position, resequencing
    /// siblings that share the new endpoint. Used for drag-and-drop filing
    /// (`structure::contained_in`) and re-targeting citations
    /// (`reference::to`).
    #[rpc(post = "/entity/relation/move", no_tool)]
    async fn move_relation(
        &self,
        ctx: &RequestContext,
        request: MoveRelationRequest,
    ) -> anyhow::Result<()>;
}
