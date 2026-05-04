//! Storage types
//!
//! Shared types used by storage traits and implementations.

pub mod asset;
pub mod blob;
pub mod content_block;
pub mod conversation;
pub mod entity;
pub mod stored;
pub mod temporal;
pub mod user;
pub mod vault;

// Re-exports for convenience
pub use asset::Asset;
pub use blob::BlobHash;
pub use content_block::{ContentBlock, ContentOrigin, ContentType, OriginKind};
pub use conversation::{Message, MessageWithContent, Span, Turn, TurnWithContent};
pub use entity::{Entity, EntityRangeQuery, EntityRelation, EntityType, RelationType};
pub use stored::{stored, stored_editable, Editable, Hashed, Keyed, Stored, StoredEditable, Timestamped};
pub use temporal::ActivitySummary;
pub use user::User;
pub use vault::{VaultConflict, VaultConflictReason, VaultFile, VaultSyncStatus};
