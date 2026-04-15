use super::chat::api::ListModelsResponse;
use super::chat::model::GeminiChatModel;
use super::embedding::GeminiEmbeddingProvider;
use crate::{ChatModel, ModelProvider};
use crate::client::Client;
use async_trait::async_trait;
use reqwest::header;
use std::sync::Arc;

pub struct GeminiProvider {
    client: Client,
    base_url: String,
}

const API_VERSION: &str = "v1beta";

impl GeminiProvider {
    pub fn default(api_key: &str) -> Self {
        Self::with_base_url("https://generativelanguage.googleapis.com", api_key)
    }

    /// Create a provider with a custom base URL (e.g., for proxying).
    /// The API version path (/v1beta) is automatically appended.
    pub fn new(base_url: &str, api_key: &str) -> Self {
        Self::with_base_url(base_url, api_key)
    }

    /// Create an embedding provider using this provider's client and base URL.
    pub fn create_embedding_provider(&self, model: &str) -> GeminiEmbeddingProvider {
        GeminiEmbeddingProvider::new(self.client.clone(), self.base_url.clone(), model.to_string())
    }

    fn with_base_url(base_url: &str, api_key: &str) -> Self {
        let mut headers = header::HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());
        headers.insert("x-goog-api-key", api_key.parse().unwrap());
        let base_url = base_url.trim_end_matches('/');
        GeminiProvider {
            client: Client::with_headers(headers),
            base_url: format!("{}/{}", base_url, API_VERSION),
        }
    }
}

#[async_trait]
impl ModelProvider for GeminiProvider {
    async fn list_models(&self) -> anyhow::Result<Vec<crate::ModelDefinition>> {
        let mut all = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let url = match &page_token {
                Some(token) => format!("{}/models?pageSize=100&pageToken={token}", self.base_url),
                None => format!("{}/models?pageSize=100", self.base_url),
            };
            let response: ListModelsResponse = self.client.get(&url).await?;
            all.extend(response.models.into_iter().map(crate::ModelDefinition::from));
            match response.next_page_token {
                Some(token) if !token.is_empty() => page_token = Some(token),
                _ => break,
            }
        }
        Ok(all)
    }

    fn create_chat_model(&self, model_name: &str) -> Option<Arc<dyn ChatModel + Send + Sync>> {
        Some(Arc::new(GeminiChatModel::new(
            self.client.clone(),
            self.base_url.clone(),
            model_name.to_string(),
        )))
    }
}
