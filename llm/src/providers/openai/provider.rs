use crate::client::Client;
use crate::ModelProvider;
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};

use super::chat::api::ListModelsResponse;
use super::chat::OpenAIChatModel;

#[derive(Clone)]
pub struct OpenAIProvider {
    client: Client,
    base_url: String,
}

impl OpenAIProvider {
    pub fn default(api_key: &str) -> Self {
        Self::new("https://api.openai.com/v1", api_key)
    }

    pub fn new(base_url: &str, api_key: &str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key))
                .expect("Invalid API key format"),
        );

        OpenAIProvider {
            client: Client::with_headers(headers),
            base_url: base_url.to_string(),
        }
    }

    fn models_url(&self) -> String {
        format!("{}/models", self.base_url)
    }
}

#[async_trait]
impl ModelProvider for OpenAIProvider {
    type ModelType = OpenAIChatModel;

    async fn list_models(&self) -> anyhow::Result<Vec<crate::ModelDefinition>> {
        let response: ListModelsResponse = self.client.get(self.models_url()).await?;
        Ok(response.data.into_iter().map(|m| m.into()).collect())
    }

    fn create_chat_model(&self, model_name: &str) -> Option<Self::ModelType> {
        Some(OpenAIChatModel::new(
            self.client.clone(),
            self.base_url.clone(),
            model_name.to_string(),
        ))
    }
}
