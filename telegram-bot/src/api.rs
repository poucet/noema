//! Minimal Telegram Bot API client.
//!
//! The bot only needs long polling and text messages, so this keeps the
//! integration on top of the existing workspace reqwest dependency instead of
//! adding another runtime framework.

use anyhow::Context as _;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Clone)]
pub struct TelegramApi {
    client: reqwest::Client,
    base_url: String,
}

impl TelegramApi {
    pub fn new(token: impl Into<String>) -> Self {
        let token = token.into();
        Self {
            client: reqwest::Client::new(),
            base_url: format!("https://api.telegram.org/bot{token}"),
        }
    }

    pub async fn get_me(&self) -> anyhow::Result<User> {
        self.post("getMe", &serde_json::json!({})).await
    }

    pub async fn delete_webhook(&self) -> anyhow::Result<bool> {
        self.post("deleteWebhook", &serde_json::json!({})).await
    }

    pub async fn get_updates(
        &self,
        offset: Option<i64>,
        timeout_secs: u64,
    ) -> anyhow::Result<Vec<Update>> {
        let body = GetUpdatesRequest {
            offset,
            timeout: timeout_secs,
            allowed_updates: vec!["message"],
        };
        self.post("getUpdates", &body).await
    }

    pub async fn send_message(
        &self,
        chat_id: i64,
        thread_id: Option<i64>,
        text: impl Into<String>,
    ) -> anyhow::Result<Message> {
        let body = SendMessageRequest {
            chat_id,
            message_thread_id: thread_id,
            text: text.into(),
        };
        self.post("sendMessage", &body).await
    }

    pub async fn send_chat_action(
        &self,
        chat_id: i64,
        thread_id: Option<i64>,
        action: ChatAction,
    ) -> anyhow::Result<bool> {
        let body = SendChatActionRequest {
            chat_id,
            message_thread_id: thread_id,
            action,
        };
        self.post("sendChatAction", &body).await
    }

    pub async fn set_my_commands(&self, commands: Vec<BotCommand>) -> anyhow::Result<bool> {
        self.post("setMyCommands", &SetMyCommandsRequest { commands })
            .await
    }

    async fn post<T, B>(&self, method: &str, body: &B) -> anyhow::Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let url = format!("{}/{method}", self.base_url);
        let resp = self.client.post(url).json(body).send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        let envelope: ApiEnvelope<T> = serde_json::from_str(&text).with_context(|| {
            format!("Telegram API returned non-JSON response ({status}): {text}")
        })?;

        if envelope.ok {
            envelope
                .result
                .ok_or_else(|| anyhow::anyhow!("Telegram API response missing result"))
        } else {
            let description = envelope
                .description
                .unwrap_or_else(|| "unknown Telegram API error".to_string());
            match envelope.error_code {
                Some(code) => anyhow::bail!("Telegram API error {code}: {description}"),
                None => anyhow::bail!("Telegram API error: {description}"),
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
    error_code: Option<i64>,
}

#[derive(Debug, Serialize)]
struct GetUpdatesRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    offset: Option<i64>,
    timeout: u64,
    allowed_updates: Vec<&'a str>,
}

#[derive(Debug, Serialize)]
struct SendMessageRequest {
    chat_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_thread_id: Option<i64>,
    text: String,
}

#[derive(Debug, Serialize)]
struct SendChatActionRequest {
    chat_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_thread_id: Option<i64>,
    action: ChatAction,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatAction {
    Typing,
}

#[derive(Clone, Debug, Serialize)]
pub struct BotCommand {
    pub command: String,
    pub description: String,
}

impl BotCommand {
    pub fn new(command: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            description: description.into(),
        }
    }
}

#[derive(Debug, Serialize)]
struct SetMyCommandsRequest {
    commands: Vec<BotCommand>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Update {
    pub update_id: i64,
    pub message: Option<Message>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Message {
    pub message_id: i64,
    pub message_thread_id: Option<i64>,
    pub from: Option<User>,
    pub chat: Chat,
    pub text: Option<String>,
    pub reply_to_message: Option<Box<Message>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Chat {
    pub id: i64,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: Option<String>,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

impl Chat {
    pub fn is_private(&self) -> bool {
        self.kind == "private"
    }

    pub fn is_group(&self) -> bool {
        self.kind == "group" || self.kind == "supergroup"
    }

    pub fn display_name(&self) -> String {
        if let Some(title) = &self.title {
            return title.clone();
        }
        if let Some(username) = &self.username {
            return format!("@{username}");
        }
        let mut parts = Vec::new();
        if let Some(first) = &self.first_name {
            parts.push(first.as_str());
        }
        if let Some(last) = &self.last_name {
            parts.push(last.as_str());
        }
        if parts.is_empty() {
            self.id.to_string()
        } else {
            parts.join(" ")
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct User {
    pub id: u64,
    pub is_bot: bool,
    pub first_name: String,
    pub last_name: Option<String>,
    pub username: Option<String>,
}

impl User {
    pub fn display_name(&self) -> String {
        let mut parts = vec![self.first_name.as_str()];
        if let Some(last) = &self.last_name {
            parts.push(last.as_str());
        }
        parts.join(" ")
    }
}
