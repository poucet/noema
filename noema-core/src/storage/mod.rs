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
//! - `TurnStore` - Turn/Span/Message conversation storage
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

