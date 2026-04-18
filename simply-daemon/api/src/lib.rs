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

use std::collections::HashMap;
use std::sync::Arc;

/// Context passed to tool call handlers (from __ctx in WS reverse calls).
#[derive(Debug, Clone, Default)]
pub struct ToolCallContext {
    /// The calling user's ID (from the daemon's user resolution).
    pub user_id: Option<String>,
    /// OAuth tokens keyed by server/provider ID.
    pub tokens: HashMap<String, String>,
}

/// Handler for tool calls from the daemon.
/// Receives (name, arguments, ctx) where ctx contains user tokens from __ctx.
pub type ToolCallHandler = Arc<
    dyn Fn(String, serde_json::Value, ToolCallContext) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<serde_json::Value>> + Send>>
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

    /// Register a skill with the daemon.
    ///
    /// Registers tools + OAuth requirements. The daemon creates auth routes
    /// for each OAuth provider so users can authenticate.
    async fn register_skill(&self, skill: Arc<dyn Skill>) -> anyhow::Result<()> {
        let tools = skill.tools();
        let oauth_reqs = skill.oauth_requirements();
        let handler: ToolCallHandler = Arc::new(move |name, args, ctx| {
            let skill = Arc::clone(&skill);
            Box::pin(async move {
                let skill_ctx = skill::SkillCallContext {
                    user_id: types::UserId::from_string(ctx.user_id.as_deref().unwrap_or("anonymous")),
                    tokens: ctx.tokens,
                };
                let result = skill.call_tool(&name, args, &skill_ctx).await?;
                Ok(serde_json::to_value(&result)?)
            })
        });
        self.register_client_tools_with_oauth(tools, oauth_reqs, handler).await
    }

    /// Register tools + OAuth requirements with the daemon (low-level).
    async fn register_client_tools_with_oauth(
        &self,
        tools: Vec<llm::ToolDefinition>,
        oauth_requirements: Vec<skill::OAuthRequirement>,
        handler: ToolCallHandler,
    ) -> anyhow::Result<()> {
        // Register OAuth providers first (so auth routes exist before tools are callable)
        for req in &oauth_requirements {
            self.register_oauth_provider(req).await?;
        }
        self.register_client_tools(tools, handler).await
    }

    /// Register an OAuth provider so users can authenticate for a skill.
    /// Uses the existing MCP server infrastructure for auth routing + token storage.
    async fn register_oauth_provider(
        &self,
        req: &skill::OAuthRequirement,
    ) -> anyhow::Result<()> {
        // Check if this OAuth provider is already registered
        let servers = self.mcp().list_mcp_servers().await?;
        if servers.iter().any(|s| s.id == req.id) {
            tracing::debug!(id = %req.id, "OAuth provider already registered");
            return Ok(());
        }

        // Register as MCP server config for OAuth token management.
        // The server won't be "connected" (no real MCP endpoint) but the
        // auth routes and token store will work.
        self.mcp().add_mcp_server(AddMcpServerRequest {
            id: req.id.clone(),
            name: req.display_name.clone(),
            url: format!("skill://{}", req.id), // Not a real URL — marks this as a skill auth entry
            auth_type: "oauth".to_string(),
            auth_token: None,
            client_id: None, // Uses stored oauth_credentials by URL
            client_secret: None,
            scopes: Some(req.scopes.clone()),
        }).await?;

        tracing::info!(id = %req.id, name = %req.display_name, "registered OAuth provider for skill");
        Ok(())
    }

    /// Register tools that this client provides to the daemon (low-level).
    ///
    /// - **Embedded**: registers directly with the tool service (no network)
    /// - **Remote**: sends `tools.register` over WS, sets up reverse call handler
    ///
    /// Prefer `register_skill()` for a higher-level API.
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
