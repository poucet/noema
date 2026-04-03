//! Standard response types for the RPC framework.

use serde::{Deserialize, Serialize};

/// A binary response with raw bytes and a MIME type.
///
/// When a trait method returns `Result<BinaryResponse>`, the HTTP server
/// serves the `data` as raw bytes with `Content-Type: mime_type` instead of JSON.
/// This is detected at compile time from the return type — no annotation needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryResponse {
    #[serde(with = "crate::base64_bytes")]
    pub data: Vec<u8>,
    pub mime_type: String,
}

impl BinaryResponse {
    pub fn new(data: Vec<u8>, mime_type: impl Into<String>) -> Self {
        Self { data, mime_type: mime_type.into() }
    }
}
