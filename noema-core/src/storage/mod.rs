//! Storage abstractions for conversation persistence
//!
//! This module provides traits and implementations for storing conversations,
//! documents, users, and assets. Multiple implementations are available:
//!
//! - `MemorySession` - In-memory storage (default, no persistence)
//! - `SqliteSession` - SQLite-backed storage (requires `sqlite` feature)
//!
//! ## Storage Traits
//!
//! - `SessionStore` - Session-level storage with transactions
//! - `UserStore` - User account management
//! - `AssetStore` - Asset metadata storage
//! - `DocumentStore` - Document, tab, and revision storage
//! - `ConversationStore` - Conversation, thread, and span storage
//! - `BlobStore` - Content-addressable binary storage
mod helper;

// Session module contains traits and all session implementations

// Domain-specific storage modules
pub mod asset;
pub mod blob;
pub mod content;
pub mod conversation;
pub mod document;
pub mod session;
pub mod user;

// pub use asset::{AssetInfo, AssetStore};
// pub use blob::{BlobStore, FsBlobStore, StoredBlob};
// pub use content::{StoredContent, StoredMessage, StoredPayload, UnresolvedBlobError};
// pub use conversation::{
//     ConversationInfo, ConversationStore, SpanInfo, SpanSetInfo, SpanSetWithContent, SpanType, ThreadInfo,
// };
// pub use document::{DocumentInfo, DocumentRevisionInfo, DocumentSource, DocumentStore, DocumentTabInfo};
// pub use session::{MemorySession, MemoryTransaction, SessionStore, StorageTransaction};
// pub use user::{UserInfo, UserStore};