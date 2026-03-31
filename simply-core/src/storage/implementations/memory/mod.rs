//! In-memory storage implementations for testing
//!
//! These implementations store data in memory and are useful for unit tests
//! where you don't want to hit a real database.

mod asset;
mod blob;
mod collection;
mod document;
mod entity;
mod reference;
mod text;
mod turn;
mod user;

pub use asset::MemoryAssetStore;
pub use blob::MemoryBlobStore;
pub use collection::MemoryCollectionStore;
pub use document::MemoryDocumentStore;
pub use entity::MemoryEntityStore;
pub use reference::MemoryReferenceStore;
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
    type Turn = MemoryTurnStore;
    type User = MemoryUserStore;
    type Document = MemoryDocumentStore;
    type Entity = MemoryEntityStore;
    type Reference = MemoryReferenceStore;
    type Collection = MemoryCollectionStore;
}
