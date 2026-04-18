//! Google Docs skill — provides import tools to the daemon.
//!
//! Uses GoogleDocsClient directly (no MCP server needed).
//! Requires the `skill` feature flag.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use base64::Engine;
use serde::Deserialize;
use tracing::info;

use simply_core::skill::{Skill, SkillContext};
use simply_core::storage::coordinator::StorageCoordinator;
use simply_core::storage::ids::UserId;
use simply_core::storage::traits::{DocumentStore, Stores, StorageTypes};
use simply_core::storage::types::DocumentType;
use simply_core::storage::DocumentSource;

use llm::{ToolDefinition, ToolResultContent};

use crate::GoogleDocsClient;

/// Google Docs skill — import documents from Google Drive into the daemon.
pub struct GDocsSkill<S: StorageTypes> {
    ctx: SkillContext<S>,
}

impl<S: StorageTypes> GDocsSkill<S> {
    pub fn new(ctx: SkillContext<S>) -> Self {
        Self { ctx }
    }

    /// Get a Google Docs client using the stored OAuth token for a user.
    /// Falls back to the global MCP server's token if available.
    fn get_client(&self, access_token: &str) -> GoogleDocsClient {
        GoogleDocsClient::new(access_token.to_string())
    }

    async fn handle_import(
        &self,
        args: ImportArgs,
        user_id: &UserId,
    ) -> Result<Vec<ToolResultContent>> {
        let access_token = args.access_token
            .ok_or_else(|| anyhow::anyhow!("access_token required for Google Docs import"))?;
        let client = self.get_client(&access_token);

        info!(doc_id = %args.doc_id, "importing Google Doc");

        // Extract from Google
        let extracted = client.extract_document(&args.doc_id).await?;
        info!(
            title = %extracted.title,
            tabs = extracted.tabs.len(),
            images = extracted.images.len(),
            "extracted Google Doc"
        );

        let doc_store = self.ctx.stores.document();

        // Check if already imported
        if let Some(existing) = doc_store
            .get_document_by_source(user_id, DocumentSource::GoogleDrive, &args.doc_id)
            .await?
        {
            return Ok(vec![ToolResultContent::text(format!(
                "Document '{}' already imported (id: {}). Use gdocs_sync to update.",
                extracted.title, existing.id
            ))]);
        }

        // Create document
        let doc_id = doc_store
            .create_document(user_id, &extracted.title, DocumentType::KNOWLEDGE, DocumentSource::GoogleDrive, Some(&args.doc_id))
            .await?;

        // Store images as assets
        let mut image_id_map = std::collections::HashMap::new();
        for image in &extracted.images {
            let data_base64 = base64::engine::general_purpose::STANDARD.encode(&image.data);
            let asset_id = self.ctx.coordinator
                .store_asset(&data_base64, &image.mime_type)
                .await?;
            image_id_map.insert(image.object_id.clone(), asset_id.into_string());
        }

        // Topological sort: create parent tabs before children
        let mut id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut pending: Vec<_> = extracted.tabs.into_iter().collect();
        let mut max_passes = pending.len() + 1;

        while !pending.is_empty() && max_passes > 0 {
            max_passes -= 1;
            let mut deferred = Vec::new();
            for tab in pending.drain(..) {
                if let Some(ref parent) = tab.parent_tab_id {
                    if !id_map.contains_key(parent) {
                        deferred.push(tab);
                        continue;
                    }
                }

                let parent_tab_id = tab.parent_tab_id.as_ref()
                    .and_then(|pid| id_map.get(pid))
                    .cloned();

                // Replace image object references with asset URLs
                let mut content = tab.content_markdown.clone();
                for (object_id, blob_hash) in &image_id_map {
                    let object_ref = format!("object:{}", object_id);
                    let asset_url = format!("/api/blob/{}", blob_hash);
                    content = content.replace(&object_ref, &asset_url);
                }

                let title = match &tab.icon {
                    Some(icon) => format!("{} {}", icon, tab.title),
                    None => tab.title.clone(),
                };

                let tab_id = doc_store
                    .create_document_tab(
                        &doc_id,
                        parent_tab_id.as_ref().map(|id| simply_core::storage::ids::TabId::from_string(id.clone())).as_ref(),
                        tab.tab_index,
                        &title,
                        tab.icon.as_deref(),
                        Some(&content),
                        &[], // referenced_assets
                        Some(&simply_core::storage::ids::TabId::from_string(tab.source_tab_id.clone())),
                    )
                    .await?;

                id_map.insert(tab.source_tab_id.clone(), tab_id.as_str().to_string());
            }
            pending = deferred;
        }

        Ok(vec![ToolResultContent::text(format!(
            "Imported '{}' with {} tabs and {} images (doc_id: {})",
            extracted.title,
            id_map.len(),
            image_id_map.len(),
            doc_id,
        ))])
    }

    async fn handle_list(
        &self,
        args: ListArgs,
    ) -> Result<Vec<ToolResultContent>> {
        let access_token = args.access_token
            .ok_or_else(|| anyhow::anyhow!("access_token required for Google Docs listing"))?;
        let client = self.get_client(&access_token);

        let files = client
            .list_documents(args.query.as_deref(), args.limit.unwrap_or(20))
            .await?;

        let result: Vec<serde_json::Value> = files.into_iter().map(|f| {
            serde_json::json!({
                "id": f.id,
                "name": f.name,
                "modified_time": f.modified_time,
            })
        }).collect();

        Ok(vec![ToolResultContent::text(
            serde_json::to_string_pretty(&result)?
        )])
    }
}

#[derive(Deserialize)]
struct ImportArgs {
    doc_id: String,
    access_token: Option<String>,
}

#[derive(Deserialize)]
struct ListArgs {
    query: Option<String>,
    limit: Option<usize>,
    access_token: Option<String>,
}

#[async_trait]
impl<S: StorageTypes> Skill for GDocsSkill<S> {
    fn name(&self) -> &str {
        "gdocs"
    }

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

    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
        user_id: &UserId,
    ) -> Result<Vec<ToolResultContent>> {
        match name {
            "gdocs_import" => {
                let args: ImportArgs = serde_json::from_value(arguments)?;
                self.handle_import(args, user_id).await
            }
            "gdocs_list" => {
                let args: ListArgs = serde_json::from_value(arguments)?;
                self.handle_list(args).await
            }
            _ => anyhow::bail!("unknown tool: {name}"),
        }
    }
}

// Schema types for tool definitions (separate from runtime args to include descriptions)
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
