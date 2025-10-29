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

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Deserialize, Serialize, JsonSchema)]
    struct CalculatorInput {
        operation: String,
        a: f64,
        b: f64,
    }

    #[derive(Debug, Deserialize, Serialize, JsonSchema)]
    struct WeatherInput {
        location: String,
        unit: Option<String>,
    }

    async fn calculator_tool(args: Value) -> Result<String> {
        let input: CalculatorInput = serde_json::from_value(args)?;
        let result = match input.operation.as_str() {
            "add" => input.a + input.b,
            "subtract" => input.a - input.b,
            "multiply" => input.a * input.b,
            "divide" => {
                if input.b == 0.0 {
                    return Err(anyhow::anyhow!("Division by zero"));
                }
                input.a / input.b
            }
            _ => return Err(anyhow::anyhow!("Unknown operation: {}", input.operation)),
        };
        Ok(result.to_string())
    }

    async fn weather_tool(args: Value) -> Result<String> {
        let input: WeatherInput = serde_json::from_value(args)?;
        let unit = input.unit.unwrap_or_else(|| "celsius".to_string());
        Ok(format!(
            "The weather in {} is 22 degrees {}",
            input.location, unit
        ))
    }

    fn create_calculator_definition() -> ToolDefinition {
        let schema = schemars::schema_for!(CalculatorInput);
        ToolDefinition {
            name: "calculator".to_string(),
            description: Some("Performs basic arithmetic operations".to_string()),
            input_schema: schema,
        }
    }

    fn create_weather_definition() -> ToolDefinition {
        let schema = schemars::schema_for!(WeatherInput);
        ToolDefinition {
            name: "get_weather".to_string(),
            description: Some("Gets the current weather for a location".to_string()),
            input_schema: schema,
        }
    }

    #[tokio::test]
    async fn test_tool_registry_registration() {
        let mut registry = ToolRegistry::new();

        registry.register(create_calculator_definition(), calculator_tool);
        registry.register(create_weather_definition(), weather_tool);

        assert!(registry.has_tool("calculator"));
        assert!(registry.has_tool("get_weather"));
        assert!(!registry.has_tool("nonexistent_tool"));
    }

    #[tokio::test]
    async fn test_calculator_tool_addition() {
        let mut registry = ToolRegistry::new();
        registry.register(create_calculator_definition(), calculator_tool);

        let args = serde_json::json!({
            "operation": "add",
            "a": 5.0,
            "b": 3.0
        });

        let result = registry.call("calculator", args).await.unwrap();
        assert_eq!(result, "8");
    }

    #[tokio::test]
    async fn test_calculator_tool_division() {
        let mut registry = ToolRegistry::new();
        registry.register(create_calculator_definition(), calculator_tool);

        let args = serde_json::json!({
            "operation": "divide",
            "a": 10.0,
            "b": 2.0
        });

        let result = registry.call("calculator", args).await.unwrap();
        assert_eq!(result, "5");
    }

    #[tokio::test]
    async fn test_calculator_division_by_zero() {
        let mut registry = ToolRegistry::new();
        registry.register(create_calculator_definition(), calculator_tool);

        let args = serde_json::json!({
            "operation": "divide",
            "a": 10.0,
            "b": 0.0
        });

        let result = registry.call("calculator", args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Division by zero"));
    }

    #[tokio::test]
    async fn test_weather_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(create_weather_definition(), weather_tool);

        let args = serde_json::json!({
            "location": "San Francisco",
            "unit": "fahrenheit"
        });

        let result = registry.call("get_weather", args).await.unwrap();
        assert!(result.contains("San Francisco"));
        assert!(result.contains("fahrenheit"));
    }

    #[tokio::test]
    async fn test_weather_tool_default_unit() {
        let mut registry = ToolRegistry::new();
        registry.register(create_weather_definition(), weather_tool);

        let args = serde_json::json!({
            "location": "London"
        });

        let result = registry.call("get_weather", args).await.unwrap();
        assert!(result.contains("London"));
        assert!(result.contains("celsius"));
    }

    #[tokio::test]
    async fn test_call_nonexistent_tool() {
        let registry = ToolRegistry::new();

        let args = serde_json::json!({});
        let result = registry.call("nonexistent", args).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_get_all_definitions() {
        let mut registry = ToolRegistry::new();
        registry.register(create_calculator_definition(), calculator_tool);
        registry.register(create_weather_definition(), weather_tool);

        let definitions = registry.get_all_definitions();
        assert_eq!(definitions.len(), 2);

        let names: Vec<_> = definitions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"calculator"));
        assert!(names.contains(&"get_weather"));
    }

    #[tokio::test]
    async fn test_get_definition() {
        let mut registry = ToolRegistry::new();
        registry.register(create_calculator_definition(), calculator_tool);

        let def = registry.get_definition("calculator");
        assert!(def.is_some());
        assert_eq!(def.unwrap().name, "calculator");

        let missing = registry.get_definition("missing");
        assert!(missing.is_none());
    }
}
