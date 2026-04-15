//! Model listing and management.

use async_trait::async_trait;
use simply_rpc::RequestContext;
use super::types::{ModelInfo, ProviderInfo};

#[simply_rpc::rpc_service("model")]
#[async_trait]
pub trait ModelApi: Send + Sync {
    /// List available models from all providers.
    #[rpc(get = "/model")]
    async fn list_models(&self, ctx: RequestContext) -> anyhow::Result<Vec<ModelInfo>>;

    /// List available providers and their configuration.
    #[rpc(get = "/model/provider")]
    async fn list_providers(&self, ctx: RequestContext) -> Vec<ProviderInfo>;

    /// Get the current default model ID.
    #[rpc(get = "/model/default")]
    async fn default_model_id(&self, ctx: RequestContext) -> String;

    /// Set the default model for new sessions.
    #[rpc(put = "/model/default")]
    async fn set_default_model(&self, ctx: RequestContext, model_id: &str) -> anyhow::Result<()>;
}
