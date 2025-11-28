//! Storage abstractions for conversation persistence
//!
//! This module provides traits and implementations for storing conversations.
//! Two implementations are available:
//!
//! - `MemorySession` - In-memory storage (default, no persistence)
//! - `SqliteSession` - SQLite-backed storage (requires `sqlite` feature)
//!
//! Both implement the same `SessionStore` trait, making them interchangeable.

mod memory;
#[cfg(feature = "sqlite")]
mod sqlite;
mod traits;

pub use memory::{MemorySession, MemoryTransaction};
#[cfg(feature = "sqlite")]
pub use sqlite::{ConversationInfo, SqliteSession, SqliteStore};
pub use traits::{SessionStore, StorageTransaction};
