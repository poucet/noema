//! Storage abstractions for conversation persistence
//!
//! This module provides traits and implementations for storing conversations,
//! documents, users, and assets.
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
//! - `ConversationStore` - Conversation-level CRUD
//! - `TextStore` - Content-addressed text storage
//! - `AssetStore` - Asset metadata storage
//! - `BlobStore` - Content-addressable binary storage
//! - `DocumentStore` - Document, tab, and revision storage
//! - `UserStore` - User account management
//!
//! ## Session API
//!
//! - `Session<T: TurnStore, C: TextStore>` - DB-agnostic session with lazy content resolution
//! - `ResolvedContent` / `ResolvedMessage` - Cached resolved content
//! - `AssetResolver` - Resolution trait for assets and documents

pub(crate) mod helper;

// Type-safe ID newtypes
pub mod ids;

// StoredContent type for message content references (internal to storage layer)
pub(crate) mod content;

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

// Document resolution for RAG
pub mod document_resolver;

// ============================================================================
// Re-exports for convenience
// ============================================================================

// Traits
pub use traits::{
    AssetStore, BlobStore, ConversationStore, DocumentStore, StorageTypes, TextStore, TurnStore,
    UserStore,
};

// Types
pub use types::{
    // Asset
    Asset, StoredAsset,
    // Blob
    StoredBlob,
    // ContentBlock
    ContentBlock, ContentOrigin, ContentType, OriginKind, StoredContentBlock, StoreResult,
    // Conversation
    ConversationInfo, ForkInfo, MessageInfo, MessageRole, MessageWithContent,
    SpanInfo, SpanRole, SpanWithMessages, TurnInfo, TurnWithContent, ViewInfo, ViewSelection,
    // Document
    DocumentInfo, DocumentRevisionInfo, DocumentSource, DocumentTabInfo, FullDocumentInfo,
    // User
    UserInfo,
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
    MemoryAssetStore, MemoryBlobStore, MemoryTextStore, MemoryConversationStore,
    MemoryDocumentStore, MemoryTurnStore,
};

// Document resolution
pub use document_resolver::{DocumentFormatter, DocumentResolver};
