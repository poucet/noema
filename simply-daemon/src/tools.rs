//! Daemon tool services — expose daemon REST APIs as tools for agents.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use llm::{ToolDefinition, ToolResultContent};
use simply_rpc::{ContentPart, RestService, RouteKind};

use simply_core::ToolService;

/// Convert `ContentPart` (from RPC dispatch) to `ToolResultContent` (for LLM agents).
fn content_part_to_tool_result(part: ContentPart) -> ToolResultContent {
    match part {
        ContentPart::Json(value) => {
            ToolResultContent::text(serde_json::to_string_pretty(&value).unwrap_or_default())
        }
        ContentPart::Binary { data, mime_type } => {
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
            if mime_type.starts_with("image/") {
                ToolResultContent::image(encoded, mime_type)
            } else if mime_type.starts_with("audio/") {
                ToolResultContent::audio(encoded, mime_type)
            } else {
                ToolResultContent::text(format!("[binary: {} bytes, {}]", data.len(), mime_type))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// DaemonToolService
// ---------------------------------------------------------------------------

/// A `ToolService` that exposes daemon REST methods as tools.
///
/// Each REST method (not marked `no_tool`) becomes a tool with:
/// - Name: `{prefix}_{method_name}` (e.g. `conversation.list_conversations`)
/// - Description: from `///` doc comment on the trait method
/// - Input schema: empty for now (TODO: derive from params via RpcSchema/schemars)
pub struct DaemonToolService {
    services: Vec<Arc<dyn RestService>>,
}

impl DaemonToolService {
    pub fn new() -> Self {
        Self { services: Vec::new() }
    }

    pub fn register(mut self, svc: Arc<dyn RestService>) -> Self {
        self.services.push(svc);
        self
    }
}

#[async_trait]
impl ToolService for DaemonToolService {
    async fn get_definitions(&self) -> Vec<ToolDefinition> {
        self.services
            .iter()
            .flat_map(|svc| {
                svc.meta().routes.iter().filter_map(|rm| {
                    if rm.no_tool { return None; }
                    if !rm.is_rest() { return None; }
                    Some(ToolDefinition {
                        name: rm.method_name.to_string(),
                        description: rm.description.map(|s| s.to_string()),
                        // TODO: derive from param types via RpcSchema/schemars
                        input_schema: Default::default(),
                    })
                })
            })
            .collect()
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<Vec<ToolResultContent>> {
        for svc in &self.services {
            if let Some(result) = svc.rest_dispatch_as_content(name, arguments.clone()).await {
                let parts = result?;
                return Ok(parts.into_iter().map(content_part_to_tool_result).collect());
            }
        }
        anyhow::bail!("tool not found: {name}")
    }
}

/// A `ToolService` that combines multiple tool services.
///
/// Agent sees one merged tool list. Tool calls are routed to the first
/// service that claims the tool.
pub struct CompositeToolService {
    services: Vec<Box<dyn ToolService>>,
}

impl CompositeToolService {
    pub fn new() -> Self {
        Self { services: Vec::new() }
    }

    pub fn add(mut self, svc: impl ToolService + 'static) -> Self {
        self.services.push(Box::new(svc));
        self
    }
}

#[async_trait]
impl ToolService for CompositeToolService {
    async fn get_definitions(&self) -> Vec<ToolDefinition> {
        let mut defs = Vec::new();
        for svc in &self.services {
            defs.extend(svc.get_definitions().await);
        }
        defs
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<Vec<ToolResultContent>> {
        for svc in &self.services {
            let has_tool = svc.get_definitions().await.iter().any(|d| d.name == name);
            if has_tool {
                return svc.call_tool(name, arguments).await;
            }
        }
        anyhow::bail!("tool not found: {name}")
    }
}

impl Default for DaemonToolService {
    fn default() -> Self { Self::new() }
}

impl Default for CompositeToolService {
    fn default() -> Self { Self::new() }
}
