//! Storage abstractions for conversation persistence
//!
//! This module provides traits and implementations for storing conversations.
//! Two implementations are available:
//!
//! - `MemorySession` - In-memory storage (default, no persistence)
//! - `SqliteSession` - SQLite-backed storage (requires `sqlite` feature)
//!
//! Both implement the `SessionStore` trait, making them interchangeable.
//!
//! Additionally, `BlobStore` provides content-addressable storage for binary assets.

mod blob;
mod content;
mod memory;

#[cfg(feature = "sqlite")]
mod sqlite;

mod traits;

pub use blob::{BlobStore, StoredBlob};
pub use content::{StoredContent, StoredMessage, StoredPayload, UnresolvedBlobError};
pub use memory::{MemorySession, MemoryTransaction};

pub use blob::FsBlobStore;

#[cfg(feature = "sqlite")]
pub use sqlite::{
    ConversationInfo, DocumentInfo, DocumentRevisionInfo, DocumentSource, DocumentTabInfo,
    SpanInfo, SpanSetInfo, SpanSetWithContent, SpanType, ThreadInfo,
    SqliteSession, SqliteStore,
};
pub use traits::{SessionStore, StorageTransaction};
