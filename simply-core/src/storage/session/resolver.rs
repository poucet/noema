//! Asset resolution traits for Session
//!
//! AssetResolver resolves assets and entities for LLM context.

use anyhow::Result;
use async_trait::async_trait;
use llm::ContentBlock;

// ============================================================================
// AssetResolver - for assets and entities
// ============================================================================

/// Trait for resolving assets and entities to ContentBlocks.
///
/// Used during `Session::messages_for_llm()` to expand
/// `ResolvedContent::Asset` and `ResolvedContent::Entity` into full
/// `ContentBlock`s.
#[async_trait]
pub trait AssetResolver: Send + Sync {
    /// Fetch asset data and return as base64-encoded ContentBlock
    ///
    /// Returns ContentBlock::Image or ContentBlock::Audio depending on mime_type
    async fn resolve_asset(&self, asset_id: &str, mime_type: &str) -> Result<ContentBlock>;

    /// Format an entity's content for LLM injection (title, kind, body,
    /// descendants). Returns `ContentBlock::Text` with the assembled
    /// markdown.
    async fn resolve_entity(&self, entity_id: &str) -> Result<ContentBlock>;
}
