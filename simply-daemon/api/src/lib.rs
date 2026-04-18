//! simply-daemon-api — trait definitions for daemon services.
//!
//! This crate defines the API surface shared by:
//! - `simply-daemon` (implementations)
//! - Skills (consume API traits)
//! - Lumina and other clients (remote client)
//!
//! No implementations live here — just traits, request/response types,
//! and `#[rpc_service]` annotations that generate REST/WS dispatch.

#[macro_use] mod session;
#[macro_use] mod conversation;
#[macro_use] mod asset;
#[macro_use] mod document;
#[macro_use] mod mcp;
#[macro_use] mod oauth;
#[macro_use] mod model;
#[macro_use] mod voice;
#[macro_use] mod core;
#[macro_use] mod search;
#[macro_use] mod user;
pub mod client;
pub mod remote;
pub mod skill;
pub mod types;

pub use session::*;
pub use conversation::*;
pub use asset::*;
pub use document::*;
pub use mcp::*;
pub use oauth::*;
pub use model::*;
pub use voice::*;
pub use self::core::*;
pub use search::*;
pub use user::*;
pub use types::*;
pub use skill::{Skill, SkillCallContext, SkillFactory};
pub use remote::RemoteDaemon;
pub use simply_rpc::{BinaryResponse, BinaryUpload};

use std::sync::Arc;

/// Handler for tool calls from the daemon.
pub type ToolCallHandler = Arc<
    dyn Fn(String, serde_json::Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<serde_json::Value>> + Send>>
    + Send + Sync
>;

/// Trait providing access to all daemon API services.
///
/// Implemented by `EmbeddedDaemon` and `RemoteDaemon`.
#[async_trait::async_trait]
pub trait Daemon: Send + Sync {
    fn session(&self) -> &dyn SessionApi;
    fn conversation(&self) -> &dyn ConversationApi;
    fn document(&self) -> &dyn DocumentApi;
    fn mcp(&self) -> &dyn McpApi;
    fn oauth(&self) -> &dyn OAuthApi;
    fn model(&self) -> &dyn ModelApi;
    fn asset(&self) -> &dyn AssetApi;
    fn voice(&self) -> &dyn VoiceApi;
    fn core(&self) -> &dyn CoreApi;
    fn search(&self) -> &dyn SearchApi;
    fn user(&self) -> &dyn UserApi;
    fn tools(&self) -> &dyn simply_core::ToolService;

    /// Register tools that this client provides to the daemon.
    ///
    /// - **Embedded**: registers directly with the tool service (no network)
    /// - **Remote**: sends `tools.register` over WS, sets up reverse call handler
    ///
    /// The `handler` is called when the daemon needs to invoke one of these tools.
    async fn register_client_tools(
        &self,
        tools: Vec<llm::ToolDefinition>,
        handler: ToolCallHandler,
    ) -> anyhow::Result<()>;
}

#[cfg(all(test, feature = "ts"))]
mod ts_export {
    use ts_rs::TS;

    #[test]
    fn export_all_types() {
        use crate::*;

        SessionId::export_all().expect("SessionId");
        DaemonEvent::export_all().expect("DaemonEvent");
        InboundEvent::export_all().expect("InboundEvent");
        ConversationInfo::export_all().expect("ConversationInfo");
        EmbeddingQueueStatus::export_all().expect("EmbeddingQueueStatus");
        SessionInfo::export_all().expect("SessionInfo");
        CreateSessionOptions::export_all().expect("CreateSessionOptions");
        UserMessage::export_all().expect("UserMessage");
        SeedMessage::export_all().expect("SeedMessage");
        DocumentInfo::export_all().expect("DocumentInfo");
        DocumentDetail::export_all().expect("DocumentDetail");
        TabInfo::export_all().expect("TabInfo");
        CreateDocumentRequest::export_all().expect("CreateDocumentRequest");
        CreateTabRequest::export_all().expect("CreateTabRequest");
        UpdateTabRequest::export_all().expect("UpdateTabRequest");
        McpServerInfo::export_all().expect("McpServerInfo");
        AddMcpServerRequest::export_all().expect("AddMcpServerRequest");
        RegisterEphemeralRequest::export_all().expect("RegisterEphemeralRequest");
        UpdateMcpServerRequest::export_all().expect("UpdateMcpServerRequest");
        OAuthFlowInfo::export_all().expect("OAuthFlowInfo");
        AssetInfo::export_all().expect("AssetInfo");
        VoiceProviderInfo::export_all().expect("VoiceProviderInfo");
        SearchHit::export_all().expect("SearchHit");
        SearchRequest::export_all().expect("SearchRequest");
        ReindexStatus::export_all().expect("ReindexStatus");
    }
}
