//! Daemon REST APIs exposed as tools.
//!
//! `DaemonToolService` wraps the registered `RestService`s and serves their
//! routes as LLM-callable tools. The richer dispatch (skills, per-user MCP,
//! WS providers, etc.) lives in [`super::registry::ToolRegistry`].

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use llm::{ToolDefinition, ToolResultContent};
use simply_rpc::{BinaryResponse, RestService};

use simply_core::ToolService;

/// Convert an RPC result `Value` to `ToolResultContent`, using route metadata
/// to determine if the result is binary (image/audio) or JSON text.
fn value_to_tool_result(value: serde_json::Value, binary_response: bool) -> Vec<ToolResultContent> {
    if binary_response {
        use base64::Engine;
        return match serde_json::from_value::<BinaryResponse>(value) {
            Ok(binary) => {
                let data = base64::engine::general_purpose::STANDARD.encode(&binary.data);
                if binary.mime_type.starts_with("image/") {
                    vec![ToolResultContent::image(data, binary.mime_type)]
                } else if binary.mime_type.starts_with("audio/") {
                    vec![ToolResultContent::audio(data, binary.mime_type)]
                } else {
                    vec![ToolResultContent::text(format!("[binary: {} bytes, {}]", binary.data.len(), binary.mime_type))]
                }
            }
            Err(e) => vec![ToolResultContent::text(format!("failed to parse binary response: {e}"))],
        };
    }
    let text = serde_json::to_string_pretty(&value).unwrap_or_default();
    vec![ToolResultContent::text(text)]
}

/// Exposes daemon REST methods as tools.
///
/// Holds a `RequestContext` that's passed to every dispatch call.
/// Create a scoped instance via `with_context()` for per-user sessions.
pub struct DaemonToolService {
    services: Vec<Arc<dyn RestService>>,
    ctx: simply_rpc::RequestContext,
}

impl DaemonToolService {
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
            ctx: simply_rpc::RequestContext::default(),
        }
    }

    pub fn register(mut self, svc: Arc<dyn RestService>) -> Self {
        self.services.push(svc);
        self
    }

    /// Access the registered service list (for ServiceRouter, etc.)
    pub fn services(&self) -> &[Arc<dyn RestService>] {
        &self.services
    }

    /// Create a new DaemonToolService with the same services but a different context.
    /// Used to scope daemon tools to a specific user's session.
    pub fn with_context(&self, ctx: simply_rpc::RequestContext) -> Self {
        Self {
            services: self.services.clone(),
            ctx,
        }
    }
}

#[async_trait]
impl ToolService for DaemonToolService {
    async fn get_definitions(&self) -> Vec<ToolDefinition> {
        self.services
            .iter()
            .flat_map(|svc| {
                svc.meta().routes.iter().filter_map(|rm| {
                    if rm.no_tool || !rm.is_rest() { return None; }
                    Some(ToolDefinition {
                        name: rm.method_name.to_string(),
                        description: rm.description.map(|s| s.to_string()),
                        input_schema: (rm.tool_schema)().cloned().unwrap_or_default(),
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
            let route = svc.meta().routes.iter().find(|rm| rm.method_name == name);
            if let Some(rm) = route {
                if let Some(result) = svc.rest_dispatch_by_name(name, &self.ctx, arguments.clone()).await {
                    let value = result?;
                    return Ok(value_to_tool_result(value, rm.binary_response));
                }
            }
        }
        anyhow::bail!("tool not found: {name}")
    }
}

impl Default for DaemonToolService {
    fn default() -> Self { Self::new() }
}
