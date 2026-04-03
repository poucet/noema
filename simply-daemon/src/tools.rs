//! Daemon tool services — expose daemon REST APIs as tools for agents.

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
        if let Ok(binary) = serde_json::from_value::<BinaryResponse>(value) {
            use base64::Engine;
            let data = base64::engine::general_purpose::STANDARD.encode(&binary.data);
            if binary.mime_type.starts_with("image/") {
                return vec![ToolResultContent::image(data, binary.mime_type)];
            } else if binary.mime_type.starts_with("audio/") {
                return vec![ToolResultContent::audio(data, binary.mime_type)];
            }
            // Unknown binary — serialize as JSON text
            let text = format!("[binary: {} bytes, {}]", binary.data.len(), binary.mime_type);
            return vec![ToolResultContent::text(text)];
        }
    }
    let text = serde_json::to_string_pretty(&value).unwrap_or_default();
    vec![ToolResultContent::text(text)]
}

/// A `ToolService` that exposes daemon REST methods as tools.
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
                    if rm.no_tool || !rm.is_rest() { return None; }
                    Some(ToolDefinition {
                        name: rm.method_name.to_string(),
                        description: rm.description.map(|s| s.to_string()),
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
            let route = svc.meta().routes.iter().find(|rm| rm.method_name == name);
            if let Some(rm) = route {
                if let Some(result) = svc.rest_dispatch_by_name(name, arguments.clone()).await {
                    let value = result?;
                    return Ok(value_to_tool_result(value, rm.binary_response));
                }
            }
        }
        anyhow::bail!("tool not found: {name}")
    }
}

/// A `ToolService` that combines multiple tool services.
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
