use super::chat::model::ClaudeChatModel;
use crate::{ChatModel, ModelProvider};
use crate::client::Client;
use async_trait::async_trait;
use reqwest::header;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ModelInfo {
    // TODO: Consider parsing this into a date-time
    created_at: String,

    display_name: String,

    id: String,
}

impl From<ModelInfo> for crate::ModelDefinition {
    fn from(model: ModelInfo) -> Self {
        // Note: The Anthropic API does not provide capability metadata in the /v1/models response.
        // The API only returns: id, display_name, type, and created_at.
        // Therefore, we can only assume all Claude models support text/chat generation.
        //
        // Limitation: We cannot programmatically detect:
        // - Vision capabilities (all Claude 3+ models support vision, but this isn't indicated in the API)
        // - Embedding capabilities (Anthropic doesn't offer embedding models)
        //
        // All Claude models support text/chat as their primary capability.
        let capabilities = vec![crate::ModelCapability::Text];

        crate::ModelDefinition::new(model.id, capabilities)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ListModelsResponse {
    data: Vec<ModelInfo>,

    first_id: Option<String>,

    has_more: bool,

    last_id: Option<String>,
}

pub struct ClaudeProvider {
    client: Client,
    base_url: String,
}

const API_VERSION: &str = "v1";

impl ClaudeProvider {
    pub fn default(api_key: &str) -> Self {
        Self::with_base_url("https://api.anthropic.com", api_key)
    }

    /// Create a provider with a custom base URL (e.g., for proxying).
    /// The API version path (/v1) is automatically appended.
    pub fn new(base_url: &str, api_key: &str) -> Self {
        Self::with_base_url(base_url, api_key)
    }

    fn with_base_url(base_url: &str, api_key: &str) -> Self {
        let mut headers = header::HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());
        headers.insert("x-api-key", api_key.parse().unwrap());
        headers.insert("anthropic-version", "2023-06-01".parse().unwrap());
        let base_url = base_url.trim_end_matches('/');
        ClaudeProvider {
            client: Client::with_headers(headers),
            base_url: format!("{}/{}", base_url, API_VERSION),
        }
    }
}

#[async_trait]
impl ModelProvider for ClaudeProvider {
    async fn list_models(&self) -> anyhow::Result<Vec<crate::ModelDefinition>> {
        // TODO: Add support for pagination.
        let url = format!("{}/models", self.base_url);
        let response: ListModelsResponse = self.client.get(&url).await?;
        Ok(response.data.into_iter().map(|m| m.into()).collect())
    }

    fn create_chat_model(&self, model_name: &str) -> Option<Arc<dyn ChatModel + Send + Sync>> {
        Some(Arc::new(ClaudeChatModel::new(
            self.client.clone(),
            self.base_url.clone(),
            model_name.to_string(),
        )))
    }
}
