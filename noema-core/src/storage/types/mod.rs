//! Storage types
//!
//! Shared types used by storage traits and implementations.

pub mod asset;
pub mod blob;
pub mod collection;
pub mod content_block;
pub mod conversation;
pub mod document;
pub mod entity;
pub mod reference;
pub mod stored;
pub mod temporal;
pub mod user;

// Re-exports for convenience
pub use asset::Asset;
pub use blob::BlobHash;
pub use content_block::{ContentBlock, ContentOrigin, ContentType, OriginKind};
pub use conversation::{
    ForkInfo, Message, MessageWithContent,
    Span, Turn, TurnWithContent, View, ViewSelection,
};
pub use document::{Document, DocumentRevision, DocumentSource, DocumentTab};
pub use entity::{Entity, EntityRelation, EntityType, RelationType};
pub use collection::{
    Collection, CollectionItem, CollectionView, FieldDefinition, FieldType,
    ItemTarget, ViewConfig, ViewType,
};
pub use reference::{EntityRef, Reference};
pub use stored::{stored, stored_editable, Editable, Hashed, Keyed, Stored, StoredEditable, Timestamped};
pub use temporal::{ActivitySummary, TemporalEntity, TemporalQuery};
pub use user::User;
