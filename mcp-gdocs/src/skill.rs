//! Google Docs skill — provides import tools to the daemon.
//!
//! Uses GoogleDocsClient for Google API access.
//! Uses `Arc<dyn Daemon>` for daemon API access (document creation, asset storage).
//! Auth tokens come from `SkillCallContext` — injected by daemon, transparent to LLM.
//! Requires the `skill` feature flag.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use base64::Engine;
use serde::Deserialize;
use tracing::info;

use simply_daemon_api::{
    Daemon, Skill, SkillCallContext,
    CreateDocumentRequest, CreateTabRequest,
};
use simply_rpc::RequestContext;
use llm::{ToolDefinition, ToolResultContent};

use crate::GoogleDocsClient;

/// The server ID used to look up Google OAuth tokens in SkillCallContext.
const GOOGLE_SERVER_ID: &str = "google-docs";

/// Google Docs skill — import documents from Google Drive.
///
/// Takes `Arc<dyn Daemon>` — works with both embedded and remote daemons.
/// Google OAuth tokens come from the daemon's token store via SkillCallContext.
pub struct GDocsSkill {
    daemon: Arc<dyn Daemon>,
}

impl GDocsSkill {
    pub fn new(daemon: Arc<dyn Daemon>) -> Self {
        Self { daemon }
    }

    fn ctx_for(ctx: &SkillCallContext) -> RequestContext {
        RequestContext::with_scope(simply_rpc::Scope::user(ctx.user_id.as_str()))
    }

    fn get_google_token(ctx: &SkillCallContext) -> Result<String> {
        ctx.token(GOOGLE_SERVER_ID)
            .or_else(|| ctx.token("google"))
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!(
                "No Google OAuth token found. Please authenticate with Google first."
            ))
    }

    async fn handle_import(&self, args: ImportArgs, ctx: &SkillCallContext) -> Result<Vec<ToolResultContent>> {
        let token = Self::get_google_token(ctx)?;
        let client = GoogleDocsClient::new(token);
        let rpc_ctx = Self::ctx_for(ctx);

        info!(doc_id = %args.doc_id, "importing Google Doc");

        // Extract from Google
        let extracted = client.extract_document(&args.doc_id).await?;
        info!(title = %extracted.title, tabs = extracted.tabs.len(), images = extracted.images.len(), "extracted");

        // Create document via daemon API
        let doc = self.daemon.document().create_document(&rpc_ctx, CreateDocumentRequest {
            title: extracted.title.clone(),
            document_type: Some("knowledge".to_string()),
            content: None,
            source_id: Some(args.doc_id.clone()),
        }).await?;

        // Store images as assets via daemon API
        let mut image_id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for image in &extracted.images {
            let data_b64 = base64::engine::general_purpose::STANDARD.encode(&image.data);
            let info = self.daemon.asset().store_asset(
                &rpc_ctx,
                simply_daemon_api::BinaryUpload {
                    data: data_b64.into_bytes(),
                    mime_type: image.mime_type.clone(),
                },
            ).await?;
            image_id_map.insert(image.object_id.clone(), info.blob_hash.as_str().to_string());
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
                let parent_tab_id = tab.parent_tab_id.as_ref()
                    .and_then(|pid| id_map.get(pid))
                    .cloned();

                let mut content = tab.content_markdown.clone();
                for (oid, hash) in &image_id_map {
                    content = content.replace(&format!("object:{oid}"), &format!("/api/blob/{hash}"));
                }

                let title = match &tab.icon {
                    Some(icon) => format!("{icon} {}", tab.title),
                    None => tab.title.clone(),
                };

                let created_tab = self.daemon.document().create_tab(&rpc_ctx, &doc.id, CreateTabRequest {
                    title,
                    content: Some(content),
                    parent_tab_id: parent_tab_id,
                    tab_index: Some(tab.tab_index),
                }).await?;

                id_map.insert(tab.source_tab_id.clone(), created_tab.id.clone());
            }
            pending = deferred;
        }

        Ok(vec![ToolResultContent::text(format!(
            "Imported '{}' with {} tabs and {} images (doc_id: {})",
            extracted.title, id_map.len(), image_id_map.len(), doc.id,
        ))])
    }

    async fn handle_list(&self, args: ListArgs, ctx: &SkillCallContext) -> Result<Vec<ToolResultContent>> {
        let token = Self::get_google_token(ctx)?;
        let client = GoogleDocsClient::new(token);
        let files = client.list_documents(args.query.as_deref(), args.limit.unwrap_or(20)).await?;
        let result: Vec<serde_json::Value> = files.into_iter().map(|f| {
            serde_json::json!({ "id": f.id, "name": f.name, "modified_time": f.modified_time })
        }).collect();
        Ok(vec![ToolResultContent::text(serde_json::to_string_pretty(&result)?)])
    }
}

#[derive(Deserialize)]
struct ImportArgs { doc_id: String }

#[derive(Deserialize)]
struct ListArgs { query: Option<String>, limit: Option<usize> }

#[async_trait]
impl Skill for GDocsSkill {
    fn name(&self) -> &str { "gdocs" }

    fn oauth_requirements(&self) -> Vec<simply_daemon_api::skill::OAuthRequirement> {
        vec![simply_daemon_api::skill::OAuthRequirement {
            id: GOOGLE_SERVER_ID.to_string(),
            display_name: "Google Docs".to_string(),
            authorization_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            scopes: vec![
                "https://www.googleapis.com/auth/documents.readonly".to_string(),
                "https://www.googleapis.com/auth/drive.readonly".to_string(),
            ],
            userinfo_url: Some("https://www.googleapis.com/oauth2/v2/userinfo".to_string()),
        }]
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

    async fn call_tool(&self, name: &str, arguments: serde_json::Value, ctx: &SkillCallContext) -> Result<Vec<ToolResultContent>> {
        match name {
            "gdocs_import" => self.handle_import(serde_json::from_value(arguments)?, ctx).await,
            "gdocs_list" => self.handle_list(serde_json::from_value(arguments)?, ctx).await,
            _ => anyhow::bail!("unknown tool: {name}"),
        }
    }
}

// No access_token in tool schemas — daemon injects it via SkillCallContext
#[derive(schemars::JsonSchema)]
struct ImportToolInput {
    /// The Google Doc ID to import.
    doc_id: String,
}

#[derive(schemars::JsonSchema)]
struct ListToolInput {
    /// Optional search query to filter documents.
    query: Option<String>,
    /// Maximum number of documents to return (default: 20).
    limit: Option<usize>,
}
