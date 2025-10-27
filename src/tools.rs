use crate::api::ToolDefinition;
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

type ToolFn = Box<dyn Fn(Value) -> Pin<Box<dyn Future<Output = Result<String>> + Send>> + Send + Sync>;

pub struct ToolRegistry {
    tools: HashMap<String, (ToolDefinition, ToolFn)>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        ToolRegistry {
            tools: HashMap::new(),
        }
    }

    pub fn register<F, Fut>(&mut self, definition: ToolDefinition, handler: F)
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<String>> + Send + 'static,
    {
        let name = definition.name.clone();
        let wrapped: ToolFn = Box::new(move |args| Box::pin(handler(args)));
        self.tools.insert(name, (definition, wrapped));
    }

    pub fn get_definition(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name).map(|(def, _)| def)
    }

    pub fn get_all_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|(def, _)| def.clone()).collect()
    }

    pub async fn call(&self, name: &str, args: Value) -> Result<String> {
        match self.tools.get(name) {
            Some((_, handler)) => handler(args).await,
            None => Err(anyhow::anyhow!("Tool '{}' not found", name)),
        }
    }

    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
