//! Asset resolution traits for Session
//!
//! AssetResolver resolves assets and documents for LLM context.

use anyhow::Result;
use async_trait::async_trait;
use llm::ContentBlock;

// ============================================================================
// AssetResolver - for assets and documents
// ============================================================================

/// Trait for resolving assets and documents to ContentBlocks
///
/// This is used during Session::messages_for_llm() to resolve
/// ResolvedContent::Asset and ResolvedContent::Document to full ContentBlocks.
#[async_trait]
pub trait AssetResolver: Send + Sync {
    /// Fetch asset data and return as base64-encoded ContentBlock
    ///
    /// Returns ContentBlock::Image or ContentBlock::Audio depending on mime_type
    async fn resolve_asset(&self, asset_id: &str, mime_type: &str) -> Result<ContentBlock>;

    /// Format document content for LLM injection
    ///
    /// Uses DocumentFormatter to create formatted text content suitable
    /// for the LLM context. Returns ContentBlock::Text with formatted content.
    async fn resolve_document(&self, document_id: &str) -> Result<ContentBlock>;
}
