use async_trait::async_trait;
use serde_json::json;
use crate::{ChatModel, ChatStream, ChatMessage, ModelProvider};
use reqwest;
mod api;
use api::{GenerateContentRequest, GenerateContentResponse, Content, ListModelsResponse, Part};


pub struct GeminiProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl GeminiProvider {
    pub fn default(api_key: &str) -> Self {
        Self::new("https://generativelanguage.googleapis.com/v1beta", api_key)
    }

    pub fn new(base_url: &str, api_key: &str) -> Self {
        GeminiProvider {
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
        }
    }
}

pub struct GeminiChatModel {
    client: reqwest::Client,
    base_url: String,
    model_name: String,
    api_key: String,
}

impl GeminiChatModel {
    pub fn new(client: reqwest::Client, base_url: String, model_name: String, api_key: String) -> Self {
        GeminiChatModel {
            client,
            base_url,
            model_name,
            api_key,
        }
    }
}

#[async_trait]
impl ModelProvider for GeminiProvider {
    type ModelType = GeminiChatModel;

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        let url = format!("{}/models", self.base_url);
        let resp = self.client.get(&url)
            .header("x-goog-api-key", self.api_key.clone()).send().await;

        match resp {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<ListModelsResponse>().await {
                        Ok(models) => Ok(models.models.iter().map(|m| m.name.clone()).collect()),
                        Err(_) => Err(anyhow::anyhow!("Failed to parse response")),
                    }
                } else {
                    Err(anyhow::anyhow!("Request failed with status: {}", response.status()))
                }
            }
            Err(_) => Err(anyhow::anyhow!("Request error")),
        }
    }

    fn create_chat_model(&self, model_name: &str) -> Option<Self::ModelType> {
        Some(GeminiChatModel::new(
            self.client.clone(),
            self.base_url.clone(),
            model_name.to_string(),
            self.api_key.clone(),
        ))
    }
}


#[async_trait]
impl ChatModel for GeminiChatModel {
    async fn chat(&self, messages: Vec<ChatMessage>) -> anyhow::Result<ChatMessage> {
        let url = format!("{}/{}:generateContent", self.base_url, self.model_name);
        println!("Sending request to URL: {}", url);

        // Separate system messages because they need to go into the system_messages field.
        let system_instruction = Content {
            parts: messages.iter().filter(|m| m.role == crate::Role::System)
            .map(|m| m.into()).collect::<Vec<Part>>(),
            role: api::Role::User, // Role is ignored for system messages   
        };
        let contents = messages.iter().filter(|m| m.role != crate::Role::System)
            .map(|msg: &ChatMessage| msg.into())
            .collect::<Vec<Content>>();

        let request = GenerateContentRequest::new(contents, Some(system_instruction));
        let response = self.client.post(&url).header("x-goog-api-key", self.api_key.clone()).header("Content-Type", "application/json").json(&request).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Request failed with status: {}", response.status()));
        }

        let message = response.json::<GenerateContentResponse>().await?;
        Ok((&message.candidates.first().unwrap().content).into())
    }


    async fn stream_chat(&self, messages: Vec<ChatMessage>) -> anyhow::Result<ChatStream> {
        // TODO
        unimplemented!("Gemini streaming not yet implemented");
    }
}