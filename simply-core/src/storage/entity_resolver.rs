//! Entity resolution for `@mention` / RAG-style references in LLM chat.
//!
//! When a chat message contains `ContentBlock::EntityRef { id }`, the
//! `EntityResolver` expands it to markdown before the message reaches an
//! LLM provider. Expansion happens in two distinct phases:
//!
//! 1. **Collect** — walk `structure::contained_in` from each requested
//!    root, producing a list of reachable entity ids. Cycles and deep
//!    trees are bounded by `MAX_DEPTH` + a `seen` set.
//! 2. **Fetch** — look up each id's data (entity row + content block) in
//!    one pass. No graph walking, no ordering decisions.
//!
//! Keeping the two phases separate means the fetch path never has to
//! reason about the shape of the `contained_in` graph, and ids collected
//! can later be batched or cached independently of rendering.
//!
//! Within a single `ChatRequest`, the first reference to an entity gets
//! the full rendering; subsequent references collapse to a shorthand.

use std::collections::{HashMap, HashSet};

use askama::Template;
use async_trait::async_trait;
use futures::future::BoxFuture;
use futures::FutureExt;
use llm::{ChatRequest, ContentBlock};

use crate::storage::ids::EntityId;
use crate::storage::traits::{EntityStore, TextStore};
use crate::storage::types::RelationType;

/// A fetched entity, rendered as one `<entity>` block by the template.
/// Self-contained — no nested references, so bidirectional or cyclic
/// graphs can't produce infinite-sized records.
pub struct ResolvedEntity {
    pub id: String,
    pub kind: String,
    pub title: String,
    /// The entity's own content (empty if it has no `content_block_id`).
    pub body: String,
}

/// Resolves `EntityRef { id }` content blocks to their full text.
#[async_trait]
pub trait EntityResolver: Send + Sync {
    /// For each requested root id, return a flat DFS-ordered list of
    /// `ResolvedEntity`: the root plus each transitive
    /// `structure::contained_in` child. Missing ids map to an empty list.
    async fn resolve_entities(
        &self,
        ids: &[EntityId],
    ) -> HashMap<EntityId, Vec<ResolvedEntity>>;
}

/// Depth cap so a pathologically deep (or cyclic) `contained_in` graph
/// doesn't blow the stack or the LLM context budget.
const MAX_DEPTH: usize = 6;

pub struct StoreEntityResolver<E: EntityStore, C: TextStore> {
    entity_store: std::sync::Arc<E>,
    text_store: std::sync::Arc<C>,
}

impl<E: EntityStore + 'static, C: TextStore + 'static> StoreEntityResolver<E, C> {
    pub fn new(
        entity_store: std::sync::Arc<E>,
        text_store: std::sync::Arc<C>,
    ) -> Self {
        Self { entity_store, text_store }
    }

    /// Phase 1 — DFS-walk the `contained_in` subtree rooted at `root`,
    /// returning reachable ids in first-seen order. Does **not** fetch
    /// entity data or content blocks; that's phase 2's job.
    fn collect_subtree_ids<'a>(
        &'a self,
        root: &'a EntityId,
    ) -> BoxFuture<'a, Vec<EntityId>> {
        async move {
            let mut seen: HashSet<String> = HashSet::new();
            let mut out: Vec<EntityId> = Vec::new();
            self.walk(root, 1, &mut seen, &mut out).await;
            out
        }
        .boxed()
    }

    fn walk<'a>(
        &'a self,
        id: &'a EntityId,
        depth: usize,
        seen: &'a mut HashSet<String>,
        out: &'a mut Vec<EntityId>,
    ) -> BoxFuture<'a, ()> {
        async move {
            if depth > MAX_DEPTH {
                return;
            }
            if !seen.insert(id.as_str().to_string()) {
                return;
            }
            out.push(id.clone());

            let relation = RelationType::structure_contained_in();
            let children = self
                .entity_store
                .list_relations_to_ordered(id, &relation)
                .await
                .ok()
                .unwrap_or_default();
            for (child_id, _edge) in children {
                self.walk(&child_id, depth + 1, seen, out).await;
            }
        }
        .boxed()
    }

    /// Phase 2 — batch-fetch entities + their content blocks. Two
    /// queries total regardless of `ids.len()`. Ordering matches `ids`;
    /// missing entities are silently dropped.
    async fn fetch_many(&self, ids: &[EntityId]) -> Vec<ResolvedEntity> {
        let entities = self
            .entity_store
            .get_entities(ids)
            .await
            .unwrap_or_default();

        // Gather the content-block ids we need, then batch-fetch their text.
        let block_ids: Vec<_> = entities
            .iter()
            .filter_map(|e| e.content_block_id.clone())
            .collect();
        let texts = self
            .text_store
            .get_texts(&block_ids)
            .await
            .unwrap_or_default();

        entities
            .iter()
            .map(|e| {
                let body = e
                    .content_block_id
                    .as_ref()
                    .and_then(|b| texts.get(b).cloned())
                    .unwrap_or_default();
                ResolvedEntity {
                    id: e.id.to_string(),
                    kind: e.entity_type.as_str().to_string(),
                    title: e.name.clone().unwrap_or_else(|| "(untitled)".to_string()),
                    body,
                }
            })
            .collect()
    }
}

#[async_trait]
impl<E: EntityStore + 'static, C: TextStore + 'static> EntityResolver
    for StoreEntityResolver<E, C>
{
    async fn resolve_entities(
        &self,
        ids: &[EntityId],
    ) -> HashMap<EntityId, Vec<ResolvedEntity>> {
        let mut out = HashMap::with_capacity(ids.len());
        for root in ids {
            let subtree_ids = self.collect_subtree_ids(root).await;
            let entities = self.fetch_many(&subtree_ids).await;
            out.insert(root.clone(), entities);
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Rendering — askama templates in simply-core/templates/.
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "entity.txt", escape = "none")]
struct EntityTemplate<'a> {
    entities: &'a [ResolvedEntity],
}

#[derive(Template)]
#[template(path = "entity_shorthand.txt", escape = "none")]
struct EntityShorthandTemplate<'a> {
    id: &'a str,
    title: &'a str,
}

/// Injects resolved entities into a `ChatRequest`, replacing `EntityRef`
/// blocks with markdown text. The first occurrence of each id in the
/// request gets the full body; later occurrences collapse to a shorthand.
#[derive(Debug, Clone, Default)]
pub struct EntityFormatter;

impl EntityFormatter {
    pub fn inject(
        &self,
        request: &mut ChatRequest,
        resolved: &HashMap<EntityId, Vec<ResolvedEntity>>,
    ) {
        let mut seen: HashSet<String> = HashSet::new();
        for msg in request.messages_mut() {
            for block in &mut msg.payload.content {
                if let ContentBlock::EntityRef { id } = block {
                    let key = EntityId::from_string(id.clone());
                    let Some(entities) = resolved.get(&key) else { continue };
                    if entities.is_empty() {
                        continue;
                    }
                    let text = if seen.insert(id.clone()) {
                        self.format_full(entities)
                    } else {
                        self.format_shorthand(&entities[0])
                    };
                    *block = ContentBlock::Text { text };
                }
            }
        }
    }

    pub fn format_full(&self, entities: &[ResolvedEntity]) -> String {
        EntityTemplate { entities }
            .render()
            .unwrap_or_else(|e| format!("[render error: {e}]"))
    }

    pub fn format_shorthand(&self, entity: &ResolvedEntity) -> String {
        EntityShorthandTemplate {
            id: entity.id.as_str(),
            title: entity.title.as_str(),
        }
        .render()
        .unwrap_or_else(|e| format!("[render error: {e}]"))
    }
}
