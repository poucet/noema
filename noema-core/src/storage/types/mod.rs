//! Storage types
//!
//! Shared types used by storage traits and implementations.

pub mod asset;
pub mod blob;
pub mod content_block;
pub mod conversation;
pub mod document;
pub mod user;

// Re-exports for convenience
pub use asset::{Asset, StoredAsset};
pub use blob::StoredBlob;
pub use content_block::{
    ContentBlock, ContentOrigin, ContentType, OriginKind, StoredContentBlock, StoreResult,
};
pub use conversation::{
    ConversationInfo, ForkInfo, MessageInfo, MessageRole, MessageWithContent,
    SpanInfo, SpanRole, TurnInfo, TurnWithContent, ViewInfo, ViewSelection,
};
pub use document::{DocumentInfo, DocumentRevisionInfo, DocumentSource, DocumentTabInfo};
pub use user::UserInfo;
