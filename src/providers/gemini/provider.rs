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

impl GeminiProvider {
    pub fn default(api_key: &str) -> Self {
        Self::new("https://generativelanguage.googleapis.com/v1beta", api_key)
    }

    pub fn new(base_url: &str, api_key: &str) -> Self {
        let mut headers = header::HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());
        headers.insert("x-goog-api-key", api_key.parse().unwrap());
        GeminiProvider {
            client: Client::with_headers(headers),
            base_url: base_url.to_string(),
        }
    }
}

#[async_trait]
impl ModelProvider for GeminiProvider {
    type ModelType = GeminiChatModel;

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        let url = format!("{}/models", self.base_url);
        let response: ListModelsResponse = self.client.get(&url).await?;
        Ok(response.models.iter().map(|m| m.name.clone()).collect())
    }

    fn create_chat_model(&self, model_name: &str) -> Option<Self::ModelType> {
        Some(GeminiChatModel::new(
            self.client.clone(),
            self.base_url.clone(),
            model_name.to_string(),
        ))
    }
}
