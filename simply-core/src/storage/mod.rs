//! Storage abstractions for conversation persistence
//!
//! This module provides traits and implementations for storing
//! conversations, entities (documents, notes, tabs, assets, …), users,
//! and the content that backs them.
//!
//! ## Module Structure
//!
//! - `traits/` - All storage trait definitions
//! - `types/` - All storage type definitions
//! - `implementations/` - Storage backends (sqlite, memory, fs)
//! - `session/` - Session runtime for managing conversation state
//!
//! ## Storage Traits
//!
//! - `TurnStore` - Turn/Span/Message conversation storage
//! - `EntityStore` - Unified CRUD for entities (conversations, documents,
//!   notes, tabs, …) and `entity_relations`
//! - `TextStore` - Content-addressed text storage (`content_blocks`)
//! - `AssetStore` - Asset metadata storage
//! - `BlobStore` - Content-addressable binary storage
//! - `UserStore` - User account management
//!
//! ## Session API
//!
//! - `Session<T: TurnStore, C: TextStore>` - DB-agnostic session with
//!   lazy content resolution
//! - `ResolvedContent` / `ResolvedMessage` - Cached resolved content
//! - `AssetResolver` - Resolution trait for assets and entities
//! - `EntityResolver` - Expand `EntityRef { id }` chat refs to markdown

pub(crate) mod helper;

// Type-safe ID newtypes
pub mod ids;

// Content types (InputContent, StoredContent)
pub mod content;

// Storage coordinator for asset externalization
pub mod coordinator;

// Trait definitions
pub mod traits;

// Type definitions
pub mod types;

// Session runtime
pub mod session;

// Implementations
pub mod implementations;

// Entity resolution for chat @mentions / RAG-style expansion.
pub mod entity_resolver;

// ============================================================================
// Re-exports for convenience
// ============================================================================

// Traits
pub use traits::{
    AssetStore, BlobStore, EntityStore, StorageTypes,
    StoredEntity, StoredUser, Stores, TextStore, TurnStore, UserStore,
};

// Types
pub use types::{
    // Asset
    Asset,
    // ContentBlock
    ContentBlock, ContentOrigin, ContentType, OriginKind,
    // Conversation structure (turns, spans, messages)
    Message, MessageWithContent, Span, Turn, TurnWithContent,
    // Entity
    Entity, EntityRelation, EntityType, RelationType,
    // Stored wrappers
    Editable, Hashed, Keyed, Stored, StoredEditable, Timestamped,
    // User
    User,
};

// Session
pub use session::{AssetResolver, ResolvedContent, ResolvedMessage, Session};

// Input content (for UI → Session API)
pub use content::InputContent;

// Implementations (feature-gated)
#[cfg(feature = "sqlite")]
pub use implementations::sqlite::SqliteStore;


pub use implementations::fs::FsBlobStore;

// Memory implementations (for testing)
pub use implementations::memory::{
    MemoryAssetStore, MemoryBlobStore,
    MemoryEntityStore, MemoryStorage, MemoryTextStore, MemoryTurnStore,
    MemoryUserStore,
};

// Entity resolution
pub use entity_resolver::{
    EntityFormatter, EntityResolver, ResolvedEntity, StoreEntityResolver,
};
