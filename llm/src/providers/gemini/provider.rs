use super::chat::api::ListModelsResponse;
use super::chat::model::GeminiChatModel;
use crate::ModelProvider;
use crate::client::Client;
use async_trait::async_trait;
use reqwest::header;

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
    type ModelType = GeminiChatModel;

    async fn list_models(&self) -> anyhow::Result<Vec<crate::ModelDefinition>> {
        let url = format!("{}/models", self.base_url);
        let response: ListModelsResponse = self.client.get(&url).await?;
        Ok(response.models.into_iter().map(|m| m.into()).collect())
    }

    fn create_chat_model(&self, model_name: &str) -> Option<Self::ModelType> {
        Some(GeminiChatModel::new(
            self.client.clone(),
            self.base_url.clone(),
            model_name.to_string(),
        ))
    }
}
