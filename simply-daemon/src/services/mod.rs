//! Daemon service implementations.

mod model;
mod asset;
mod entity;
mod voice;
mod core;
mod search;
pub mod user;
pub mod tools;
pub mod ws_tools;
pub mod registry;
pub mod providers;
pub mod embedding_queue;
pub mod token_store;

pub use model::ModelService;
pub use asset::AssetService;
pub use entity::EntityService;
pub use voice::VoiceService;
pub use self::core::CoreService;
pub use search::SearchService;
pub use user::UserService;
pub use tools::DaemonToolService;
pub use registry::ToolRegistry;
pub use embedding_queue::{EmbeddingQueue, ChannelEmbeddingQueue, EmbedJob};
pub use token_store::{TransientTokenStore, McpUserToken};
