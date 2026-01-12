//! Session storage - DB-agnostic session abstraction
//!
//! This module provides:
//!
//! - `Session<S: TurnStore>` - DB-agnostic session with lazy resolution
//! - `ResolvedContent` / `ResolvedMessage` - Cached resolved content
//! - `ContentBlockResolver` / `AssetResolver` - Resolution traits
//! - `SqliteStore` - SQLite storage backend (requires `sqlite` feature)
//!
//! Session implements `ConversationContext` directly.

mod resolver;
mod session;
mod types;

#[cfg(feature = "sqlite")]
mod sqlite;

// Re-export session types
pub use resolver::{AssetResolver, ContentBlockResolver};
pub use session::{ContentStorer, Session};
pub use types::{ResolvedContent, ResolvedMessage};

#[cfg(feature = "sqlite")]
pub use sqlite::SqliteStore;
