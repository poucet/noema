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

#[macro_use] mod session;
#[macro_use] mod conversation;
#[macro_use] mod asset;
#[macro_use] mod mcp;
#[macro_use] mod oauth;
#[macro_use] mod model;
#[macro_use] mod voice;
pub mod types;

pub use session::*;
pub use conversation::*;
pub use asset::*;
pub use mcp::*;
pub use oauth::*;
pub use model::*;
pub use voice::*;
pub use types::*;

/// Convenience super-trait combining all daemon APIs.
pub trait DaemonApi: SessionApi + ConversationApi + AssetApi + McpApi + OAuthApi + ModelApi + VoiceApi {}

impl<T: SessionApi + ConversationApi + AssetApi + McpApi + OAuthApi + ModelApi + VoiceApi> DaemonApi for T {}
