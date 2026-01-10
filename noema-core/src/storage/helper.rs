//! Shared constants and utilities for storage implementations

use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

/// Get current unix timestamp in milliseconds
pub fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Compute SHA-256 hash of text content
pub fn content_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex::encode(hasher.finalize())
}
