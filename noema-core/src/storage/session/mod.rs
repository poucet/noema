//! Session storage - DB-agnostic session abstraction
//!
//! This module provides:
//!
//! - `Session<S: TurnStore>` - DB-agnostic session with lazy resolution
//! - `ResolvedContent` / `ResolvedMessage` - Cached resolved content
//! - `ContentBlockResolver` / `AssetResolver` - Resolution traits
//! - `SqliteStore` - SQLite storage backend (requires `sqlite` feature)
//!
//! For conversation CRUD operations, see `ConversationStore` in the
//! `conversation` module.

mod resolver;
mod session;
mod types;

#[cfg(feature = "sqlite")]
mod sqlite;

// Re-export session types
pub use resolver::{AssetResolver, ContentBlockResolver};
pub use session::Session;
pub use types::{PendingMessage, ResolvedContent, ResolvedMessage};

#[cfg(feature = "sqlite")]
pub use sqlite::SqliteStore;
