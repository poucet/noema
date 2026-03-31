use super::chat::api::ListModelsResponse;
use super::chat::model::OllamaChatModel;
use crate::{ChatModel, ModelProvider};
use crate::client::Client;
use async_trait::async_trait;
use std::sync::Arc;

pub struct OllamaProvider {
    client: Client,
    base_url: String,
}

impl OllamaProvider {
    pub fn default() -> Self {
        Self::new("http://localhost:11434")
    }

    pub fn new(base_url: &str) -> Self {
        OllamaProvider {
            client: Client::default(),
            base_url: base_url.to_string(),
        }
    }
}

#[async_trait]
impl ModelProvider for OllamaProvider {
    async fn list_models(&self) -> anyhow::Result<Vec<crate::ModelDefinition>> {
        let url = format!("{}/api/tags", self.base_url);
        let response: ListModelsResponse = self.client.get(&url).await?;
        Ok(response.models.into_iter().map(|m| m.into()).collect())
    }

    fn create_chat_model(&self, model_name: &str) -> Option<Arc<dyn ChatModel + Send + Sync>> {
        Some(Arc::new(OllamaChatModel::new(
            self.client.clone(),
            self.base_url.clone(),
            model_name.to_string(),
        )))
    }
}
