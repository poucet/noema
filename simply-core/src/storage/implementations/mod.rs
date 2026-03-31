//! Storage implementations
//!
//! This module contains concrete implementations of the storage traits.
//!
//! ## Available Implementations
//!
//! - `sqlite/` - SQLite-based storage (requires `sqlite` feature)
//! - `memory/` - In-memory storage for testing
//! - `fs/` - Filesystem-based blob storage
//! - `mock/` - Minimal mock stores for coordinator testing

#[cfg(feature = "sqlite")]
pub mod sqlite;

pub mod fs;
pub mod memory;
pub mod mock;
