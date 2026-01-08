//! Document resolution for RAG (Retrieval-Augmented Generation)
//!
//! This module provides the `DocumentResolver` trait for resolving document references
//! to their full content before sending to LLM providers.

use async_trait::async_trait;
use futures::future::join_all;
use llm::{ChatMessage, ChatPayload, ChatRequest, ContentBlock};
use std::sync::Arc;

use crate::storage::document::DocumentStore;

/// A resolved document with its content
#[derive(Debug, Clone)]
pub struct ResolvedDocument {
    pub id: String,
    pub title: String,
    pub content: String,
}

/// Trait for resolving document references to their content
#[async_trait]
pub trait DocumentResolver: Send + Sync {
    /// Resolve a document by ID, returning its content
    async fn resolve(&self, doc_id: &str) -> Option<ResolvedDocument>;
}

/// Document resolver backed by any DocumentStore implementation
pub struct StoreDocumentResolver<S> {
    store: Arc<S>,
}

impl<S: DocumentStore> StoreDocumentResolver<S> {
    /// Create a new resolver with the given store
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl<S: DocumentStore> DocumentResolver for StoreDocumentResolver<S> {
    async fn resolve(&self, doc_id: &str) -> Option<ResolvedDocument> {
        // Get document metadata
        let doc_info = self.store.get_document(doc_id).await.ok()??;

        // Get all tabs for this document
        let tabs = self.store.list_document_tabs(doc_id).await.ok()?;

        // Concatenate all tab content
        let content: String = tabs
            .iter()
            .filter_map(|tab| tab.content_markdown.as_ref())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n---------\n");

        Some(ResolvedDocument {
            id: doc_id.to_string(),
            title: doc_info.title,
            content,
        })
    }
}

/// Configuration for how documents are formatted when injected into the LLM context
#[derive(Debug, Clone)]
pub struct DocumentInjectionConfig {
    /// Template for wrapping a single document. Placeholders: {id}, {title}, {content}
    pub document_template: String,
    /// Template for wrapping all documents. Placeholder: {documents}
    pub wrapper_template: String,
    /// Instructions appended after the documents. Placeholder: {user_message}
    pub instructions_template: String,
}

impl Default for DocumentInjectionConfig {
    fn default() -> Self {
        Self {
            document_template: r#"<document id="{id}" title="{title}">
{content}
</document>"#.to_string(),
            wrapper_template: r#"<referenced_documents>
{documents}
</referenced_documents>"#.to_string(),
            instructions_template: r#"

When referring to information from these documents in your response, use markdown links in the format [relevant text](noema://doc/DOCUMENT_ID) where DOCUMENT_ID is the document's id from the document tags above.

{user_message}"#.to_string(),
        }
    }
}

impl DocumentInjectionConfig {
    /// Format a single document using the template
    pub fn format_document(&self, doc: &ResolvedDocument) -> String {
        self.document_template
            .replace("{id}", &doc.id)
            .replace("{title}", &doc.title)
            .replace("{content}", &doc.content)
    }

    /// Format all documents and combine with user message
    pub fn format_with_documents(&self, documents: &[ResolvedDocument], user_message: &str) -> String {
        if documents.is_empty() {
            return user_message.to_string();
        }

        let formatted_docs: Vec<String> = documents
            .iter()
            .map(|doc| self.format_document(doc))
            .collect();

        let wrapped = self.wrapper_template
            .replace("{documents}", &formatted_docs.join("\n\n"));

        let with_instructions = self.instructions_template
            .replace("{user_message}", user_message);

        format!("{}{}", wrapped, with_instructions)
    }
}

/// Resolve all DocumentRef blocks in a ChatPayload
pub async fn resolve_payload(
    payload: &mut ChatPayload,
    resolver: &dyn DocumentResolver,
    config: &DocumentInjectionConfig,
) {

    let mut doc_refs: Vec<(String, String)> = Vec::new();
    let mut other_content = Vec::new();
    let mut user_text = String::new();

    // Separate DocumentRefs from other content, collect user text
    for block in std::mem::take(&mut payload.content) {
        match block {
            ContentBlock::DocumentRef { id, title } => {
                doc_refs.push((id, title));
            }
            ContentBlock::Text { text } => {
                if !user_text.is_empty() {
                    user_text.push_str("\n\n");
                }
                user_text.push_str(&text);
            }
            other => other_content.push(other),
        }
    }

    // Resolve all documents in parallel
    let resolve_futures = doc_refs.iter().map(|(id, title)| async {
        let id = id.clone();
        let title = title.clone();
        match resolver.resolve(&id).await {
            Some(doc) => doc,
            None => ResolvedDocument {
                id,
                title: title.clone(),
                content: format!("[Document '{}' could not be loaded]", title),
            },
        }
    });
    let resolved_docs: Vec<ResolvedDocument> = join_all(resolve_futures).await;

    // Build new content
    let mut new_content = Vec::new();

    // Add resolved documents + user message as a single text block
    if !resolved_docs.is_empty() || !user_text.is_empty() {
        let combined_text = config.format_with_documents(&resolved_docs, &user_text);
        new_content.push(ContentBlock::Text { text: combined_text });
    }

    // Add any other content (images, audio, tool calls, etc.)
    new_content.extend(other_content);

    payload.content = new_content;
}

/// Resolve all DocumentRef blocks in a ChatMessage
pub async fn resolve_message(
    message: &mut ChatMessage,
    resolver: &dyn DocumentResolver,
    config: &DocumentInjectionConfig,
) {
    resolve_payload(&mut message.payload, resolver, config).await;
}

/// Resolve all DocumentRef blocks in a ChatRequest
pub async fn resolve_request(
    request: &mut ChatRequest,
    resolver: &dyn DocumentResolver,
    config: &DocumentInjectionConfig,
) {
    for msg in request.messages_mut() {
        resolve_message(msg, resolver, config).await;
    }
}

/// Check if a ChatPayload contains any DocumentRef blocks
pub fn payload_has_document_refs(payload: &ChatPayload) -> bool {
    payload.content.iter().any(|block| matches!(block, ContentBlock::DocumentRef { .. }))
}

/// Check if a ChatRequest contains any DocumentRef blocks
pub fn request_has_document_refs(request: &ChatRequest) -> bool {
    request.messages().iter().any(|msg| payload_has_document_refs(&msg.payload))
}

// ============================================================================
// Backwards Compatibility
// ============================================================================

/// Type alias for backwards compatibility
#[cfg(feature = "sqlite")]
pub type SqliteDocumentResolver = StoreDocumentResolver<crate::storage::session::SqliteStore>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_format() {
        let config = DocumentInjectionConfig::default();
        let doc = ResolvedDocument {
            id: "doc-123".to_string(),
            title: "Test Document".to_string(),
            content: "This is the document content.".to_string(),
        };

        let formatted = config.format_document(&doc);
        assert!(formatted.contains("doc-123"));
        assert!(formatted.contains("Test Document"));
        assert!(formatted.contains("This is the document content."));
    }

    #[test]
    fn test_format_with_multiple_documents() {
        let config = DocumentInjectionConfig::default();
        let docs = vec![
            ResolvedDocument {
                id: "doc-1".to_string(),
                title: "First Doc".to_string(),
                content: "Content 1".to_string(),
            },
            ResolvedDocument {
                id: "doc-2".to_string(),
                title: "Second Doc".to_string(),
                content: "Content 2".to_string(),
            },
        ];

        let result = config.format_with_documents(&docs, "What do these documents say?");
        assert!(result.contains("doc-1"));
        assert!(result.contains("doc-2"));
        assert!(result.contains("What do these documents say?"));
    }

    #[test]
    fn test_format_empty_documents() {
        let config = DocumentInjectionConfig::default();
        let result = config.format_with_documents(&[], "Just a user message");
        assert_eq!(result, "Just a user message");
    }
}
