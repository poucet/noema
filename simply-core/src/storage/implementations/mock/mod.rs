//! Mock storage implementations for testing
//!
//! These implementations are designed for use in coordinator tests,
//! providing minimal stub implementations that return `unimplemented!()`.

mod asset;
mod blob;
mod collection;
mod document;
mod entity;
mod reference;
mod text;
mod turn;
mod types;
mod user;

pub use asset::MockAssetStore;
pub use blob::MockBlobStore;
pub use collection::MockCollectionStore;
pub use document::MockDocumentStore;
pub use entity::MockEntityStore;
pub use reference::MockReferenceStore;
pub use text::MockTextStore;
pub use turn::MockTurnStore;
pub use types::MockStorage;
pub use user::MockUserStore;
