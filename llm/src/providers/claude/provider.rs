use super::chat::model::ClaudeChatModel;
use crate::ModelProvider;
use crate::client::Client;
use async_trait::async_trait;
use reqwest::header;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ModelInfo {
    // TODO: Consider parsing this into a date-time
    created_at: String,

    display_name: String,

    id: String,
}

impl From<ModelInfo> for crate::ModelDefinition {
    fn from(model: ModelInfo) -> Self {
        // All Claude models support text/chat
        crate::ModelDefinition::text_model(model.id)
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

impl ClaudeProvider {
    pub fn default(api_key: &str) -> Self {
        Self::new("https://api.anthropic.com/v1", api_key)
    }

    pub fn new(base_url: &str, api_key: &str) -> Self {
        let mut headers = header::HeaderMap::new();
        headers.insert("content-type", "application/json".parse().unwrap());
        headers.insert("x-api-key", api_key.parse().unwrap());
        headers.insert("anthropic-version", "2023-06-01".parse().unwrap());
        ClaudeProvider {
            client: Client::with_headers(headers),
            base_url: base_url.to_string(),
        }
    }
}

#[async_trait]
impl ModelProvider for ClaudeProvider {
    type ModelType = ClaudeChatModel;

    async fn list_models(&self) -> anyhow::Result<Vec<crate::ModelDefinition>> {
        // TODO: Add support for pagination.
        let url = format!("{}/models", self.base_url);
        let response: ListModelsResponse = self.client.get(&url).await?;
        Ok(response.data.into_iter().map(|m| m.into()).collect())
    }

    fn create_chat_model(&self, model_name: &str) -> Option<Self::ModelType> {
        Some(ClaudeChatModel::new(
            self.client.clone(),
            self.base_url.clone(),
            model_name.to_string(),
        ))
    }
}
