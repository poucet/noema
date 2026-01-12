//! Storage implementations
//!
//! This module contains concrete implementations of the storage traits.
//!
//! ## Available Implementations
//!
//! - `sqlite/` - SQLite-based storage (requires `sqlite` feature)
//! - `memory/` - In-memory storage for testing
//! - `fs/` - Filesystem-based blob storage

#[cfg(feature = "sqlite")]
pub mod sqlite;

pub mod memory;
pub mod fs;
