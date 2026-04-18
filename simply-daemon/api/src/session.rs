//! Session lifecycle and event streaming.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use simply_rpc::RequestContext;
#[cfg(feature = "ts")]
use ts_rs::TS;
use tokio::sync::broadcast;

use crate::types::{DaemonEvent, InboundEvent, InputContent, SessionId};

pub use simply_core::Persistence;

/// Options when creating a new session.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "admin/src/lib/generated/types/"))]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionOptions {
    pub persistence: Option<Persistence>,
    pub system_prompt: Option<String>,
    pub model_id: Option<String>,
    #[serde(default)]
    pub seed: Vec<SeedMessage>,
}

/// Information about a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "admin/src/lib/generated/types/"))]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: SessionId,
    pub persistence: Persistence,
    pub model_id: String,
    pub created_at: String,
}

/// A user message sent to a session.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "admin/src/lib/generated/types/"))]
pub struct UserMessage {
    pub content: Vec<InputContent>,
}

/// A seed message.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "ts", derive(TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "admin/src/lib/generated/types/"))]
pub struct SeedMessage {
    pub role: llm::Role,
    pub content: Vec<InputContent>,
}

#[simply_rpc::rpc_service("session")]
#[async_trait]
pub trait SessionApi: Send + Sync {
    #[rpc(stream = "/session/new")]
    async fn create_session(
        &self,
        ctx: &RequestContext,
        options: CreateSessionOptions,
    ) -> anyhow::Result<(SessionInfo, broadcast::Receiver<DaemonEvent>)>;

    #[rpc(stream = "/session/{session_id}/subscribe")]
    async fn subscribe_session(
        &self,
        ctx: &RequestContext,
        session_id: &SessionId,
    ) -> anyhow::Result<broadcast::Receiver<DaemonEvent>>;

    #[rpc(get = "/session")]
    async fn list_sessions(&self, ctx: &RequestContext) -> anyhow::Result<Vec<SessionInfo>>;

    #[rpc(post = "/session/{session_id}/message")]
    async fn send_message(
        &self,
        ctx: &RequestContext,
        session_id: &SessionId,
        message: UserMessage,
    ) -> anyhow::Result<()>;

    #[rpc(put = "/session/{session_id}/model")]
    async fn set_model(&self, ctx: &RequestContext, session_id: &SessionId, model_id: &str) -> anyhow::Result<()>;

    #[rpc(delete = "/session/{session_id}")]
    async fn close_session(&self, ctx: &RequestContext, session_id: &SessionId) -> anyhow::Result<()>;

    #[rpc(delete = "/session")]
    async fn close_all_sessions(&self) -> anyhow::Result<()>;

    #[rpc(post = "/session/event")]
    async fn push_event(&self, ctx: &RequestContext, event: InboundEvent) -> anyhow::Result<()>;
}
