//! Storage trait definitions
//!
//! All storage traits are defined here, with implementations in `implementations/`.

mod asset;
mod blob;
mod content_block;
mod conversation;
mod document;
mod turn;
mod user;

pub use asset::AssetStore;
pub use blob::BlobStore;
pub use content_block::ContentBlockStore;
pub use conversation::ConversationStore;
pub use document::DocumentStore;
pub use turn::TurnStore;
pub use user::UserStore;
