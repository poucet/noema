//! Document storage types
//!
//! Types for the Episteme-compatible document model.

use std::str::FromStr;

/// Document source type (matches episteme)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentSource {
    GoogleDrive,
    AiGenerated,
    UserCreated,
}

impl ToString for DocumentSource {
    fn to_string(&self) -> String {
        match self {
            DocumentSource::GoogleDrive => "google_drive".to_string(),
            DocumentSource::AiGenerated => "ai_generated".to_string(),
            DocumentSource::UserCreated => "user_created".to_string(),
        }
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
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub source: DocumentSource,
    pub source_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct DocumentTabInfo {
    pub id: String,
    pub document_id: String,
    pub parent_tab_id: Option<String>,
    pub tab_index: i32,
    pub title: String,
    pub icon: Option<String>,
    pub content_markdown: Option<String>,
    pub referenced_assets: Vec<String>,
    pub source_tab_id: Option<String>,
    pub current_revision_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct DocumentRevisionInfo {
    pub id: String,
    pub tab_id: String,
    pub revision_number: i32,
    pub parent_revision_id: Option<String>,
    pub content_markdown: String,
    pub content_hash: String,
    pub referenced_assets: Vec<String>,
    pub created_at: i64,
    pub created_by: String,
}

#[derive(Debug, Clone)]
pub struct FullDocumentInfo {
    pub document: DocumentInfo,
    pub tabs: Vec<DocumentTabInfo>,
}
