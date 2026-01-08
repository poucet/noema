//! Shared constants and utilities for storage implementations

use std::time::{SystemTime, UNIX_EPOCH};

/// Get current unix timestamp in milliseconds
pub fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
