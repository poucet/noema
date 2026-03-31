//! Document resolution for RAG (Retrieval-Augmented Generation)
//!
//! This module provides the `DocumentResolver` trait for resolving document references
//! to their full content before sending to LLM providers.

use std::collections::{HashMap, HashSet};

use askama::Template;
use async_trait::async_trait;
use futures::future::join_all;
use llm::{ChatRequest, ContentBlock};

use crate::storage::ids::{DocumentId, TabId};
use crate::storage::traits::DocumentStore;
use crate::storage::types::{Document, DocumentTab, StoredEditable};

/// A resolved document with its content and tabs
pub struct ResolvedDocument {
    pub document: StoredEditable<DocumentId, Document>,
    pub tabs: Vec<StoredEditable<TabId, DocumentTab>>,
}

/// Trait for resolving document references to their content
#[async_trait]
pub trait DocumentResolver: Send + Sync {
    /// Resolve all document IDs to their full content
    async fn resolve_documents(&self, doc_ids: &[DocumentId]) -> HashMap<DocumentId, ResolvedDocument>;
}

#[async_trait]
impl<S: DocumentStore> DocumentResolver for S {
    async fn resolve_documents(&self, doc_ids: &[DocumentId]) -> HashMap<DocumentId, ResolvedDocument> {
        join_all(doc_ids.iter().map(|id| async move {
            let result = async {
                let document = self.get_document(id).await.ok()??;
                let tabs = self.list_document_tabs(id).await.ok()?;
                Some(ResolvedDocument { document, tabs })
            }
            .await;
            (id.clone(), result)
        }))
        .await
        .into_iter()
        .filter_map(|(id, doc_opt)| doc_opt.map(|doc| (id, doc)))
        .collect()
    }
}

/// Tab data for the document template
struct TabData<'a> {
    icon: &'a str,
    title: &'a str,
    content: &'a str,
}

/// Template for rendering a full document
#[derive(Template)]
#[template(path = "document.txt")]
struct DocumentTemplate<'a> {
    id: &'a str,
    title: &'a str,
    tabs: Vec<TabData<'a>>,
}

/// Template for rendering a shorthand document reference
#[derive(Template)]
#[template(path = "document_shorthand.txt")]
struct DocumentShorthandTemplate<'a> {
    id: &'a str,
    title: &'a str,
}

/// Formats documents for injection into LLM context
#[derive(Debug, Clone, Default)]
pub struct DocumentFormatter;

impl DocumentFormatter {
    /// Inject resolved documents into a ChatRequest, replacing DocumentRef blocks with formatted text
    pub fn inject_documents(
        &self,
        request: &mut ChatRequest,
        resolved_docs: &HashMap<DocumentId, ResolvedDocument>,
    ) {
        // Track which documents have already been expanded (first reference gets full content)
        let mut expanded_docs: HashSet<String> = HashSet::new();

        for msg in request.messages_mut() {
            for block in &mut msg.payload.content {
                if let ContentBlock::DocumentRef { id } = block {
                    let doc_id = DocumentId::from_string(id.clone());
                    if let Some(doc) = resolved_docs.get(&doc_id) {
                        let formatted = if expanded_docs.insert(id.clone()) {
                            // First reference: include full content
                            self.format_document(doc)
                        } else {
                            // Subsequent references: use shorthand
                            self.format_document_shorthand(doc)
                        };
                        *block = ContentBlock::Text { text: formatted };
                    }
                }
            }
        }
    }

    /// Format a single document as markdown with full content (for first reference)
    pub fn format_document(&self, doc: &ResolvedDocument) -> String {
        let tabs: Vec<TabData> = doc
            .tabs
            .iter()
            .map(|tab| TabData {
                icon: tab.icon.as_deref().unwrap_or("📄"),
                title: &tab.title,
                content: tab.content_markdown.as_deref().unwrap_or(""),
            })
            .collect();

        let template = DocumentTemplate {
            id: doc.document.id.as_str(),
            title: &doc.document.title,
            tabs,
        };

        template.render().expect("document template should render")
    }

    /// Format a shorthand reference (for subsequent mentions of the same document)
    pub fn format_document_shorthand(&self, doc: &ResolvedDocument) -> String {
        let template = DocumentShorthandTemplate {
            id: doc.document.id.as_str(),
            title: &doc.document.title,
        };

        template
            .render()
            .expect("shorthand template should render")
    }
}
