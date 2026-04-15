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
    /// Composite tool service — includes daemon REST tools + MCP tools.
    fn tools(&self) -> &dyn simply_core::ToolService;
}
