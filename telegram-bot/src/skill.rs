//! Telegram skill tools registered with the daemon.

use async_trait::async_trait;
use rmcp::model::{CallToolResult, Content};
use schemars::JsonSchema;
use serde::Deserialize;
use simply_daemon_api::{RequestContext, Skill, ToolDefinition};

use crate::api::TelegramApi;

#[derive(Clone)]
pub struct TelegramSkill {
    api: TelegramApi,
}

impl TelegramSkill {
    pub fn new(api: TelegramApi) -> Self {
        Self { api }
    }

    async fn send_message(
        &self,
        input: SendMessageInput,
        ctx: &RequestContext,
    ) -> anyhow::Result<CallToolResult> {
        let chat_id = match input.chat_id {
            Some(id) => id,
            None => metadata_i64(ctx, "telegram.chat_id").ok_or_else(|| {
                anyhow::anyhow!(
                    "no telegram.chat_id in conversation context; pass chat_id explicitly"
                )
            })?,
        };
        let thread_id = input
            .message_thread_id
            .or_else(|| metadata_i64(ctx, "telegram.message_thread_id"));

        let sent = self
            .api
            .send_message(chat_id, thread_id, input.text)
            .await?;
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Sent Telegram message {} to chat {}",
            sent.message_id, sent.chat.id
        ))]))
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SendMessageInput {
    /// Telegram chat ID. Omit to use the current Telegram conversation.
    pub chat_id: Option<i64>,
    /// Telegram forum topic/thread ID. Omit to use the current thread, if any.
    pub message_thread_id: Option<i64>,
    /// Message text to send.
    pub text: String,
}

#[async_trait]
impl Skill for TelegramSkill {
    fn name(&self) -> &str {
        "telegram"
    }

    fn tools(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            name: "telegram_send_message".to_string(),
            description: Some(
                "Send a plain text message to a Telegram chat. If chat_id is omitted, sends to the current Telegram conversation.".to_string(),
            ),
            input_schema: schemars::schema_for!(SendMessageInput),
        }]
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
        ctx: &RequestContext,
    ) -> anyhow::Result<CallToolResult> {
        match name {
            "telegram_send_message" => {
                self.send_message(serde_json::from_value(arguments)?, ctx)
                    .await
            }
            other => anyhow::bail!("unknown Telegram tool: {other}"),
        }
    }
}

fn metadata_i64(ctx: &RequestContext, key: &str) -> Option<i64> {
    let value = ctx.metadata.get(key)?;
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}
