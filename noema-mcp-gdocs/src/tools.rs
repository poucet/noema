//! MCP Tools for Google Docs

use crate::google_api::{ExtractedDocument, ExtractedImage, ExtractedTab, GoogleDocsClient};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rmcp::{
    handler::server::ServerHandler,
    model::*,
    service::{RequestContext, RoleServer},
    ErrorData as McpError,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// MCP Server for Google Docs
#[derive(Clone)]
pub struct GoogleDocsServer {
    /// Current access token (set from Authorization header)
    access_token: Arc<RwLock<Option<String>>>,
}

impl GoogleDocsServer {
    pub fn new() -> Self {
        Self {
            access_token: Arc::new(RwLock::new(None)),
        }
    }

    async fn get_client(&self) -> Option<GoogleDocsClient> {
        self.access_token
            .read()
            .await
            .as_ref()
            .map(|token| GoogleDocsClient::new(token.clone()))
    }

    pub async fn set_access_token(&self, token: String) {
        debug!("Setting access token");
        // Remove "Bearer " prefix if present
        let token = token.strip_prefix("Bearer ").unwrap_or(&token).to_string();
        *self.access_token.write().await = Some(token);
    }

    fn get_tools() -> Vec<Tool> {
        fn make_schema(value: serde_json::Value) -> Arc<serde_json::Map<String, serde_json::Value>> {
            match value {
                serde_json::Value::Object(map) => Arc::new(map),
                _ => Arc::new(serde_json::Map::new()),
            }
        }

        vec![
            Tool {
                name: "gdocs_list".into(),
                title: None,
                description: Some("List Google Docs from the user's Drive".into()),
                input_schema: make_schema(json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Optional search query to filter documents"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of documents to return (default: 20, max: 100)",
                            "default": 20
                        }
                    }
                })),
                annotations: None,
                output_schema: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "gdocs_extract".into(),
                title: None,
                description: Some(
                    "Extract a Google Doc with all tabs and images. Returns raw tab content (HTML) \
                    and base64-encoded images for storage by noema-core."
                        .into(),
                ),
                input_schema: make_schema(json!({
                    "type": "object",
                    "properties": {
                        "doc_id": {
                            "type": "string",
                            "description": "The Google Doc ID"
                        }
                    },
                    "required": ["doc_id"]
                })),
                annotations: None,
                output_schema: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "gdocs_get_content".into(),
                title: None,
                description: Some(
                    "Get the content of a Google Doc as markdown (simple export, no multi-tab support)"
                        .into(),
                ),
                input_schema: make_schema(json!({
                    "type": "object",
                    "properties": {
                        "doc_id": {
                            "type": "string",
                            "description": "The Google Doc ID"
                        },
                        "format": {
                            "type": "string",
                            "enum": ["text", "markdown"],
                            "description": "Output format (default: markdown)",
                            "default": "markdown"
                        }
                    },
                    "required": ["doc_id"]
                })),
                annotations: None,
                output_schema: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: "gdocs_get_info".into(),
                title: None,
                description: Some("Get metadata about a Google Doc".into()),
                input_schema: make_schema(json!({
                    "type": "object",
                    "properties": {
                        "doc_id": {
                            "type": "string",
                            "description": "The Google Doc ID"
                        }
                    },
                    "required": ["doc_id"]
                })),
                annotations: None,
                output_schema: None,
                icons: None,
                meta: None,
            },
        ]
    }
}

impl Default for GoogleDocsServer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct ListArgs {
    query: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ExtractArgs {
    doc_id: String,
}

#[derive(Debug, Deserialize)]
struct GetContentArgs {
    doc_id: String,
    format: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GetInfoArgs {
    doc_id: String,
}

/// Response format for gdocs_extract - raw data for noema-core to process
#[derive(Debug, Serialize)]
struct ExtractResponse {
    doc_id: String,
    title: String,
    tabs: Vec<TabResponse>,
    images: Vec<ImageResponse>,
}

#[derive(Debug, Serialize)]
struct TabResponse {
    source_tab_id: String,
    title: String,
    icon: Option<String>,
    content_html: String,
    parent_tab_id: Option<String>,
    tab_index: i32,
}

#[derive(Debug, Serialize)]
struct ImageResponse {
    object_id: String,
    data_base64: String,
    mime_type: String,
}

impl From<ExtractedTab> for TabResponse {
    fn from(tab: ExtractedTab) -> Self {
        TabResponse {
            source_tab_id: tab.source_tab_id,
            title: tab.title,
            icon: tab.icon,
            content_html: tab.content_html,
            parent_tab_id: tab.parent_tab_id,
            tab_index: tab.tab_index,
        }
    }
}

impl From<ExtractedImage> for ImageResponse {
    fn from(img: ExtractedImage) -> Self {
        ImageResponse {
            object_id: img.object_id,
            data_base64: BASE64.encode(&img.data),
            mime_type: img.mime_type,
        }
    }
}

impl From<ExtractedDocument> for ExtractResponse {
    fn from(doc: ExtractedDocument) -> Self {
        ExtractResponse {
            doc_id: doc.doc_id,
            title: doc.title,
            tabs: doc.tabs.into_iter().map(TabResponse::from).collect(),
            images: doc.images.into_iter().map(ImageResponse::from).collect(),
        }
    }
}

impl ServerHandler for GoogleDocsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Google Docs MCP server for Noema. Provides tools to list, read, and extract Google Docs \
                with full multi-tab and image support."
                    .into(),
            ),
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult {
            tools: Self::get_tools(),
            next_cursor: None,
        }))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        async move {
            let name = request.name.as_ref();
            let arguments = request.arguments.clone().unwrap_or_default();

            info!("Calling tool: {} with args: {:?}", name, arguments);

            let client = match self.get_client().await {
                Some(c) => c,
                None => {
                    return Ok(CallToolResult::error(vec![Content::text(
                        "Error: Not authenticated. Please complete OAuth flow first.",
                    )]));
                }
            };

            match name {
                "gdocs_list" => {
                    let args: ListArgs = match serde_json::from_value(serde_json::Value::Object(arguments)) {
                        Ok(a) => a,
                        Err(e) => {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "Invalid arguments: {}",
                                e
                            ))]));
                        }
                    };

                    match client
                        .list_documents(args.query.as_deref(), args.limit.unwrap_or(20))
                        .await
                    {
                        Ok(files) => {
                            let result: Vec<serde_json::Value> = files
                                .into_iter()
                                .map(|f| {
                                    json!({
                                        "id": f.id,
                                        "name": f.name,
                                        "modified_time": f.modified_time,
                                        "created_time": f.created_time,
                                    })
                                })
                                .collect();

                            Ok(CallToolResult::success(vec![Content::text(
                                serde_json::to_string_pretty(&result).unwrap_or_default(),
                            )]))
                        }
                        Err(e) => {
                            error!("Error listing documents: {}", e);
                            Ok(CallToolResult::error(vec![Content::text(format!(
                                "Error listing documents: {}",
                                e
                            ))]))
                        }
                    }
                }

                "gdocs_extract" => {
                    let args: ExtractArgs = match serde_json::from_value(serde_json::Value::Object(arguments)) {
                        Ok(a) => a,
                        Err(e) => {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "Invalid arguments: {}",
                                e
                            ))]));
                        }
                    };

                    match client.extract_document(&args.doc_id).await {
                        Ok(doc) => {
                            let response: ExtractResponse = doc.into();
                            Ok(CallToolResult::success(vec![Content::text(
                                serde_json::to_string(&response).unwrap_or_default(),
                            )]))
                        }
                        Err(e) => {
                            error!("Error extracting document: {}", e);
                            Ok(CallToolResult::error(vec![Content::text(format!(
                                "Error extracting document: {}",
                                e
                            ))]))
                        }
                    }
                }

                "gdocs_get_content" => {
                    let args: GetContentArgs = match serde_json::from_value(serde_json::Value::Object(arguments)) {
                        Ok(a) => a,
                        Err(e) => {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "Invalid arguments: {}",
                                e
                            ))]));
                        }
                    };

                    let format = args.format.as_deref().unwrap_or("markdown");
                    let result = if format == "text" {
                        client.get_document_as_text(&args.doc_id).await
                    } else {
                        client.get_document_as_markdown(&args.doc_id).await
                    };

                    match result {
                        Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
                        Err(e) => {
                            error!("Error getting document content: {}", e);
                            Ok(CallToolResult::error(vec![Content::text(format!(
                                "Error getting document content: {}",
                                e
                            ))]))
                        }
                    }
                }

                "gdocs_get_info" => {
                    let args: GetInfoArgs = match serde_json::from_value(serde_json::Value::Object(arguments)) {
                        Ok(a) => a,
                        Err(e) => {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "Invalid arguments: {}",
                                e
                            ))]));
                        }
                    };

                    match client.get_document_info(&args.doc_id).await {
                        Ok(info) => {
                            let result = json!({
                                "id": info.id,
                                "title": info.title,
                                "revision_id": info.revision_id,
                            });

                            Ok(CallToolResult::success(vec![Content::text(
                                serde_json::to_string_pretty(&result).unwrap_or_default(),
                            )]))
                        }
                        Err(e) => {
                            error!("Error getting document info: {}", e);
                            Ok(CallToolResult::error(vec![Content::text(format!(
                                "Error getting document info: {}",
                                e
                            ))]))
                        }
                    }
                }

                _ => Ok(CallToolResult::error(vec![Content::text(format!(
                    "Unknown tool: {}",
                    name
                ))])),
            }
        }
    }
}
