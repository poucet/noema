//! In-memory storage implementations for testing
//!
//! These implementations store data in memory and are useful for unit tests
//! where you don't want to hit a real database.

mod asset;
mod blob;
mod conversation;
mod document;
mod text;
mod turn;
mod user;

pub use asset::MemoryAssetStore;
pub use blob::MemoryBlobStore;
pub use conversation::MemoryConversationStore;
pub use document::MemoryDocumentStore;
pub use text::MemoryTextStore;
pub use turn::MemoryTurnStore;
pub use user::MemoryUserStore;

use crate::storage::traits::StorageTypes;

/// In-memory storage types bundled together for testing
pub struct MemoryStorage;

impl StorageTypes for MemoryStorage {
    type Blob = MemoryBlobStore;
    type Asset = MemoryAssetStore;
    type Text = MemoryTextStore;
    type Conversation = MemoryConversationStore;
    type Turn = MemoryTurnStore;
    type User = MemoryUserStore;
    type Document = MemoryDocumentStore;
}
