//! Model listing and management.

use async_trait::async_trait;
use super::types::{ModelInfo, ProviderInfo};

#[simply_rpc::rpc_service("model")]
#[async_trait]
pub trait ModelApi: Send + Sync {
    /// List available models from all providers.
    async fn list_models(&self) -> anyhow::Result<Vec<ModelInfo>>;

    /// List available providers and their configuration.
    async fn list_providers(&self) -> Vec<ProviderInfo>;

    /// Get the current default model ID.
    async fn default_model_id(&self) -> String;

    /// Set the default model for new sessions.
    async fn set_default_model(&self, model_id: &str) -> anyhow::Result<()>;
}
