//! Session storage - DB-agnostic session abstraction
//!
//! This module provides:
//!
//! - `Session<S: TurnStore>` - DB-agnostic session with lazy resolution
//! - `ResolvedContent` / `ResolvedMessage` - Cached resolved content
//! - `AssetResolver` - Resolution trait for assets and documents
//!
//! Session implements `ConversationContext` directly.
//! Text resolution uses `ContentBlockStore::require_text()` directly.
//!
//! For SQLite storage, use `storage::SqliteStore`.

mod resolver;
mod session;
mod types;

// Re-export session types
pub use resolver::AssetResolver;
pub use session::{ContentStorer, Session};
pub use types::{ResolvedContent, ResolvedMessage};
