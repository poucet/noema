//! Storage abstractions for conversation persistence
//!
//! This module provides traits and implementations for storing conversations.
//! Two implementations are available:
//!
//! - `MemorySession` - In-memory storage (default, no persistence)
//! - `SqliteSession` - SQLite-backed storage (requires `sqlite` feature)
//!
//! Both implement the same `SessionStore` trait, making them interchangeable.
//!
//! Additionally, `BlobStore` provides content-addressable storage for binary assets.

mod blob;
mod content;
mod memory;
#[cfg(feature = "sqlite")]
mod sqlite;
mod traits;

pub use blob::{BlobStore, StoredBlob};
pub use content::{StoredContent, StoredPayload, UnresolvedBlobError};
pub use memory::{MemorySession, MemoryTransaction};
#[cfg(feature = "sqlite")]
pub use sqlite::{
    ConversationInfo, DocumentInfo, DocumentRevisionInfo, DocumentSource, DocumentTabInfo,
    SpanInfo, SpanSetInfo, SpanSetWithContent, SpanType,
    SqliteSession, SqliteStore, StoredMessage,
};
pub use traits::{SessionStore, StorageTransaction};
