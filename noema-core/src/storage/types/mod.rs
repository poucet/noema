//! Storage types
//!
//! Shared types used by storage traits and implementations.

pub mod asset;
pub mod blob;
pub mod content_block;
pub mod conversation;
pub mod document;
pub mod stored;
pub mod user;

// Re-exports for convenience
pub use asset::Asset;
pub use blob::BlobHash;
pub use content_block::{ContentBlock, ContentOrigin, ContentType, OriginKind};
pub use conversation::{
    Conversation, ForkInfo, Message, MessageRole, MessageWithContent,
    Span, Turn, TurnWithContent, View, ViewSelection,
};
pub use document::{Document, DocumentRevision, DocumentSource, DocumentTab};
pub use stored::{stored, stored_editable, Editable, Hashed, Keyed, Stored, StoredEditable, Timestamped};
pub use user::User;
