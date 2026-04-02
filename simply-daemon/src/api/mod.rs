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

mod session;
mod conversation;
mod asset;
mod mcp;
mod oauth;
mod model;
mod voice;
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
