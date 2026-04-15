//! DaemonApi — the core traits that all daemon consumers depend on.
//!
//! Split into focused traits:
//! - [`SessionApi`] — session lifecycle and event streaming
//! - [`ConversationApi`] — persistent conversation entity CRUD
//! - [`AssetApi`] — binary content upload
//! - [`McpApi`] — MCP service registration and tool discovery
//! - [`OAuthApi`] — OAuth flow management
//! - [`ModelApi`] — model listing and management
//! - [`VoiceApi`] — voice pipeline
//! - [`SearchApi`] — semantic search over embedded documents

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
pub use simply_rpc::{BinaryResponse, BinaryUpload};

/// Trait providing access to all daemon API services.
///
/// Implemented by `EmbeddedDaemon` (returns inner services directly)
/// and `RemoteDaemon` (returns `self` for each, since it implements all traits via RPC).
///
/// Consumers call `daemon.model().list_models()`, `daemon.session().create_session()`, etc.
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
    /// Composite tool service — includes daemon REST tools + MCP tools (global, no user context).
    fn tools(&self) -> &dyn simply_core::ToolService;
}

#[cfg(test)]
mod ts_export {
    use ts_rs::TS;

    /// Export all API types to TypeScript via ts-rs.
    ///
    /// Run with: cargo test -p simply-daemon ts_export
    /// Output goes to: admin/src/lib/generated/types/
    #[test]
    fn export_all_types() {
        // Daemon-owned types
        super::types::SessionId::export_all().expect("SessionId");
        super::types::DaemonEvent::export_all().expect("DaemonEvent");
        super::types::InboundEvent::export_all().expect("InboundEvent");
        super::types::ConversationInfo::export_all().expect("ConversationInfo");

        // Session types
        super::session::SessionInfo::export_all().expect("SessionInfo");
        super::session::CreateSessionOptions::export_all().expect("CreateSessionOptions");
        super::session::UserMessage::export_all().expect("UserMessage");
        super::session::SeedMessage::export_all().expect("SeedMessage");

        // Document types
        super::document::DocumentInfo::export_all().expect("DocumentInfo");
        super::document::DocumentDetail::export_all().expect("DocumentDetail");
        super::document::TabInfo::export_all().expect("TabInfo");
        super::document::CreateDocumentRequest::export_all().expect("CreateDocumentRequest");
        super::document::CreateTabRequest::export_all().expect("CreateTabRequest");
        super::document::UpdateTabRequest::export_all().expect("UpdateTabRequest");

        // MCP types
        super::mcp::McpServerInfo::export_all().expect("McpServerInfo");
        super::mcp::AddMcpServerRequest::export_all().expect("AddMcpServerRequest");
        super::mcp::RegisterEphemeralRequest::export_all().expect("RegisterEphemeralRequest");
        super::mcp::UpdateMcpServerRequest::export_all().expect("UpdateMcpServerRequest");

        // OAuth types
        super::oauth::OAuthFlowInfo::export_all().expect("OAuthFlowInfo");

        // Asset types
        super::asset::AssetInfo::export_all().expect("AssetInfo");

        // Voice types
        super::voice::VoiceProviderInfo::export_all().expect("VoiceProviderInfo");

        // Search types
        super::search::SearchHit::export_all().expect("SearchHit");
        super::search::SearchRequest::export_all().expect("SearchRequest");
        super::search::ReindexStatus::export_all().expect("ReindexStatus");

        // Embedding queue
        crate::embedding_queue::EmbeddingQueueStatus::export_all().expect("EmbeddingQueueStatus");
    }
}
