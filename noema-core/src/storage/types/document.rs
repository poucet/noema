//! Document storage types
//!
//! Types for the Episteme-compatible document model.

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

#[derive(Debug, Clone)]
pub struct DocumentInfo {
    pub id: DocumentId,
    pub user_id: UserId,
    pub title: String,
    pub source: DocumentSource,
    pub source_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct DocumentTabInfo {
    pub id: TabId,
    pub document_id: DocumentId,
    pub parent_tab_id: Option<TabId>,
    pub tab_index: i32,
    pub title: String,
    pub icon: Option<String>,
    pub content_markdown: Option<String>,
    pub referenced_assets: Vec<AssetId>,
    pub source_tab_id: Option<TabId>,
    pub current_revision_id: Option<RevisionId>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct DocumentRevisionInfo {
    pub id: RevisionId,
    pub tab_id: TabId,
    pub revision_number: i32,
    pub parent_revision_id: Option<RevisionId>,
    pub content_markdown: String,
    pub content_hash: String,
    pub referenced_assets: Vec<AssetId>,
    pub created_at: i64,
    pub created_by: UserId,
}
