//! Daemon tool services — expose daemon REST APIs as tools for agents.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use llm::{ToolDefinition, ToolResultContent};
use simply_rpc::{RestService, RouteKind};

use simply_core::ToolService;

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
                        // TODO 3.E: derive from param types via RpcSchema/schemars
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
            if let Some(result) = svc.rest_dispatch_by_name(name, arguments.clone()).await {
                match result {
                    Ok(value) => {
                        let text = serde_json::to_string_pretty(&value)?;
                        return Ok(vec![ToolResultContent::text(text)]);
                    }
                    Err(e) => return Err(e),
                }
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
            // Check if this service has the tool
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
