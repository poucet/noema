use crate::client::Client;
use crate::{ChatModel, ModelProvider};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use std::sync::Arc;

use super::chat::api::ListModelsResponse;
use super::chat::OpenAIChatModel;

#[derive(Clone)]
pub struct OpenAIProvider {
    client: Client,
    base_url: String,
}

const API_VERSION: &str = "v1";

impl OpenAIProvider {
    pub fn default(api_key: &str) -> Self {
        Self::with_base_url("https://api.openai.com", api_key)
    }

    /// Create a provider with a custom base URL (e.g., for proxying).
    /// The API version path (/v1) is automatically appended.
    pub fn new(base_url: &str, api_key: &str) -> Self {
        Self::with_base_url(base_url, api_key)
    }

    fn with_base_url(base_url: &str, api_key: &str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key))
                .expect("Invalid API key format"),
        );

        let base_url = base_url.trim_end_matches('/');
        OpenAIProvider {
            client: Client::with_headers(headers),
            base_url: format!("{}/{}", base_url, API_VERSION),
        }
    }

    fn models_url(&self) -> String {
        format!("{}/models", self.base_url)
    }
}

#[async_trait]
impl ModelProvider for OpenAIProvider {
    async fn list_models(&self) -> anyhow::Result<Vec<crate::ModelDefinition>> {
        let response: ListModelsResponse = self.client.get(self.models_url()).await?;
        Ok(response.data.into_iter().map(|m| m.into()).collect())
    }

    fn create_chat_model(&self, model_name: &str) -> Option<Arc<dyn ChatModel + Send + Sync>> {
        Some(Arc::new(OpenAIChatModel::new(
            self.client.clone(),
            self.base_url.clone(),
            model_name.to_string(),
        )))
    }
}
