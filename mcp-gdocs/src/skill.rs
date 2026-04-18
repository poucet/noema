//! Google Docs skill — provides import tools to the daemon.
//!
//! Uses GoogleDocsClient directly (no MCP server needed).
//! Accesses daemon via trait objects — no dependency on daemon internals.
//! Requires the `skill` feature flag.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use base64::Engine;
use serde::Deserialize;
use tracing::info;

use simply_daemon_api::skill::Skill;
use simply_daemon_api::types::UserId;
use llm::{ToolDefinition, ToolResultContent};

use crate::GoogleDocsClient;

// ---------------------------------------------------------------------------
// Trait objects the skill needs — daemon provides adapters
// ---------------------------------------------------------------------------

/// Document operations the skill needs.
#[async_trait]
pub trait SkillDocumentApi: Send + Sync {
    async fn create_document(&self, user_id: &UserId, title: &str, source_id: Option<&str>) -> Result<String>;
    async fn create_tab(&self, doc_id: &str, title: &str, content: &str, parent_tab_id: Option<&str>, tab_index: i32) -> Result<String>;
    async fn find_by_source(&self, user_id: &UserId, source_id: &str) -> Result<Option<String>>;
}

/// Asset storage the skill needs.
#[async_trait]
pub trait SkillAssetApi: Send + Sync {
    async fn store_asset(&self, base64_data: &str, mime_type: &str) -> Result<String>;
}

// ---------------------------------------------------------------------------
// GDocsSkill
// ---------------------------------------------------------------------------

pub struct GDocsSkill {
    documents: Arc<dyn SkillDocumentApi>,
    assets: Arc<dyn SkillAssetApi>,
}

impl GDocsSkill {
    pub fn new(documents: Arc<dyn SkillDocumentApi>, assets: Arc<dyn SkillAssetApi>) -> Self {
        Self { documents, assets }
    }

    async fn handle_import(&self, args: ImportArgs, user_id: &UserId) -> Result<Vec<ToolResultContent>> {
        let access_token = args.access_token
            .ok_or_else(|| anyhow::anyhow!("access_token required for Google Docs import"))?;
        let client = GoogleDocsClient::new(access_token);

        info!(doc_id = %args.doc_id, "importing Google Doc");

        if let Some(existing_id) = self.documents.find_by_source(user_id, &args.doc_id).await? {
            return Ok(vec![ToolResultContent::text(format!(
                "Document already imported (id: {existing_id}). Use gdocs_sync to update.",
            ))]);
        }

        let extracted = client.extract_document(&args.doc_id).await?;
        info!(title = %extracted.title, tabs = extracted.tabs.len(), images = extracted.images.len(), "extracted");

        let doc_id = self.documents.create_document(user_id, &extracted.title, Some(&args.doc_id)).await?;

        // Store images
        let mut image_id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for image in &extracted.images {
            let data_b64 = base64::engine::general_purpose::STANDARD.encode(&image.data);
            let blob_hash = self.assets.store_asset(&data_b64, &image.mime_type).await?;
            image_id_map.insert(image.object_id.clone(), blob_hash);
        }

        // Topological sort: parents first
        let mut id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut pending: Vec<_> = extracted.tabs.into_iter().collect();
        let mut max_passes = pending.len() + 1;

        while !pending.is_empty() && max_passes > 0 {
            max_passes -= 1;
            let mut deferred = Vec::new();
            for tab in pending.drain(..) {
                if let Some(ref parent) = tab.parent_tab_id {
                    if !id_map.contains_key(parent) { deferred.push(tab); continue; }
                }
                let parent_tab_id = tab.parent_tab_id.as_ref().and_then(|pid| id_map.get(pid)).map(|s| s.as_str());
                let mut content = tab.content_markdown.clone();
                for (oid, hash) in &image_id_map {
                    content = content.replace(&format!("object:{oid}"), &format!("/api/blob/{hash}"));
                }
                let title = match &tab.icon {
                    Some(icon) => format!("{icon} {}", tab.title),
                    None => tab.title.clone(),
                };
                let tab_id = self.documents.create_tab(&doc_id, &title, &content, parent_tab_id, tab.tab_index).await?;
                id_map.insert(tab.source_tab_id.clone(), tab_id);
            }
            pending = deferred;
        }

        Ok(vec![ToolResultContent::text(format!(
            "Imported '{}' with {} tabs and {} images (doc_id: {doc_id})",
            extracted.title, id_map.len(), image_id_map.len(),
        ))])
    }

    async fn handle_list(&self, args: ListArgs) -> Result<Vec<ToolResultContent>> {
        let access_token = args.access_token
            .ok_or_else(|| anyhow::anyhow!("access_token required"))?;
        let client = GoogleDocsClient::new(access_token);
        let files = client.list_documents(args.query.as_deref(), args.limit.unwrap_or(20)).await?;
        let result: Vec<serde_json::Value> = files.into_iter().map(|f| {
            serde_json::json!({ "id": f.id, "name": f.name, "modified_time": f.modified_time })
        }).collect();
        Ok(vec![ToolResultContent::text(serde_json::to_string_pretty(&result)?)])
    }
}

#[derive(Deserialize)]
struct ImportArgs { doc_id: String, access_token: Option<String> }

#[derive(Deserialize)]
struct ListArgs { query: Option<String>, limit: Option<usize>, access_token: Option<String> }

#[async_trait]
impl Skill for GDocsSkill {
    fn name(&self) -> &str { "gdocs" }

    fn tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "gdocs_import".to_string(),
                description: Some("Import a Google Doc into the document store with all tabs and images.".to_string()),
                input_schema: schemars::schema_for!(ImportToolInput),
            },
            ToolDefinition {
                name: "gdocs_list".to_string(),
                description: Some("List Google Docs from the user's Drive.".to_string()),
                input_schema: schemars::schema_for!(ListToolInput),
            },
        ]
    }

    async fn call_tool(&self, name: &str, arguments: serde_json::Value, user_id: &UserId) -> Result<Vec<ToolResultContent>> {
        match name {
            "gdocs_import" => self.handle_import(serde_json::from_value(arguments)?, user_id).await,
            "gdocs_list" => self.handle_list(serde_json::from_value(arguments)?).await,
            _ => anyhow::bail!("unknown tool: {name}"),
        }
    }
}

#[derive(schemars::JsonSchema)]
struct ImportToolInput {
    /// The Google Doc ID to import.
    doc_id: String,
    /// OAuth access token for Google APIs.
    access_token: Option<String>,
}

#[derive(schemars::JsonSchema)]
struct ListToolInput {
    /// Optional search query to filter documents.
    query: Option<String>,
    /// Maximum number of documents to return (default: 20).
    limit: Option<usize>,
    /// OAuth access token for Google APIs.
    access_token: Option<String>,
}
