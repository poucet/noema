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

/// Trait providing access to all daemon API services.
///
/// Implemented by `EmbeddedDaemon` and `RemoteDaemon`.
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
}
