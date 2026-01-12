//! Storage abstractions for conversation persistence
//!
//! This module provides traits and implementations for storing conversations,
//! documents, users, and assets.
//!
//! ## Session API
//!
//! - `Session<S: TurnStore>` - DB-agnostic session with lazy content resolution
//! - `ResolvedContent` / `ResolvedMessage` - Cached resolved content
//! - `ContentBlockResolver` / `AssetResolver` - Resolution traits
//!
//! ## Storage Traits
//!
//! - `TurnStore` - Turn/Span/Message conversation storage
//! - `ConversationStore` - Conversation-level CRUD
//! - `UserStore` - User account management
//! - `AssetStore` - Asset metadata storage
//! - `DocumentStore` - Document, tab, and revision storage
//! - `BlobStore` - Content-addressable binary storage
mod helper;

// Type-safe ID newtypes
pub mod ids;

// Session module contains traits and all session implementations

// Domain-specific storage modules
pub mod asset;
pub mod blob;
pub mod content;
pub mod content_block;
pub mod conversation;
pub mod coordinator;
pub mod document;
pub mod session;
pub mod user;

