//! Storage trait definitions
//!
//! All storage traits are defined here, with implementations in `implementations/`.

mod asset;
mod blob;
mod conversation;
mod document;
mod text;
mod turn;
mod user;

pub use asset::AssetStore;
pub use blob::BlobStore;
pub use conversation::ConversationStore;
pub use document::DocumentStore;
pub use text::TextStore;
pub use turn::TurnStore;
pub use user::UserStore;
