//! Document storage types
//!
//! Types for the Episteme-compatible document model.
//!
//! Documents and tabs are editable, so they use `StoredEditable<Id, T>`.
//! Revisions are immutable, so they use `Stored<Id, T>`.

use std::str::FromStr;

use crate::storage::ids::{AssetId, DocumentId, RevisionId, TabId, UserId};

/// Document source type (matches episteme)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentSource {
    GoogleDrive,
    AiGenerated,
    UserCreated,
}

impl DocumentSource {
    /// Get static string representation (zero allocation)
    pub const fn as_str(&self) -> &'static str {
        match self {
            DocumentSource::GoogleDrive => "google_drive",
            DocumentSource::AiGenerated => "ai_generated",
            DocumentSource::UserCreated => "user_created",
        }
    }
}

impl std::fmt::Display for DocumentSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DocumentSource {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "google_drive" => Ok(DocumentSource::GoogleDrive),
            "ai_generated" => Ok(DocumentSource::AiGenerated),
            "user_created" => Ok(DocumentSource::UserCreated),
            _ => Err(format!("{s} is not a valid DocumentSource")),
        }
    }
}

/// Core document data
///
/// Use with `StoredEditable<DocumentId, Document>` for the full stored representation.
#[derive(Debug, Clone)]
pub struct Document {
    pub user_id: UserId,
    pub title: String,
    pub source: DocumentSource,
    pub source_id: Option<String>,
}

impl Document {
    /// Create a new document
    pub fn new(user_id: UserId, title: impl Into<String>, source: DocumentSource) -> Self {
        Self {
            user_id,
            title: title.into(),
            source,
            source_id: None,
        }
    }

    /// Set the source ID
    pub fn with_source_id(mut self, source_id: impl Into<String>) -> Self {
        self.source_id = Some(source_id.into());
        self
    }
}

/// Core document tab data
///
/// Use with `StoredEditable<TabId, DocumentTab>` for the full stored representation.
#[derive(Debug, Clone)]
pub struct DocumentTab {
    pub document_id: DocumentId,
    pub parent_tab_id: Option<TabId>,
    pub tab_index: i32,
    pub title: String,
    pub icon: Option<String>,
    pub content_markdown: Option<String>,
    pub referenced_assets: Vec<AssetId>,
    pub source_tab_id: Option<TabId>,
    pub current_revision_id: Option<RevisionId>,
}

impl DocumentTab {
    /// Create a new document tab
    pub fn new(document_id: DocumentId, tab_index: i32, title: impl Into<String>) -> Self {
        Self {
            document_id,
            parent_tab_id: None,
            tab_index,
            title: title.into(),
            icon: None,
            content_markdown: None,
            referenced_assets: Vec::new(),
            source_tab_id: None,
            current_revision_id: None,
        }
    }
}

/// Core document revision data
///
/// Use with `Stored<RevisionId, DocumentRevision>` for the full stored representation.
/// Revisions are immutable (no Editable wrapper needed).
#[derive(Debug, Clone)]
pub struct DocumentRevision {
    pub tab_id: TabId,
    pub revision_number: i32,
    pub parent_revision_id: Option<RevisionId>,
    pub content_markdown: String,
    pub content_hash: String,
    pub referenced_assets: Vec<AssetId>,
    pub created_by: UserId,
}

impl DocumentRevision {
    /// Create a new document revision
    pub fn new(
        tab_id: TabId,
        revision_number: i32,
        content_markdown: impl Into<String>,
        content_hash: impl Into<String>,
        created_by: UserId,
    ) -> Self {
        Self {
            tab_id,
            revision_number,
            parent_revision_id: None,
            content_markdown: content_markdown.into(),
            content_hash: content_hash.into(),
            referenced_assets: Vec::new(),
            created_by,
        }
    }

    /// Set the parent revision
    pub fn with_parent(mut self, parent_revision_id: RevisionId) -> Self {
        self.parent_revision_id = Some(parent_revision_id);
        self
    }
}
