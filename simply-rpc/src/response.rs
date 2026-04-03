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

/// A binary upload — raw bytes with a MIME type.
///
/// When a trait method has a `BinaryUpload` parameter, the HTTP server reads
/// the raw request body as bytes and the `Content-Type` header as the MIME type.
/// Other parameters go in the query string.
///
/// Over WebSocket, the data is base64-encoded in JSON (transparent to the caller).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryUpload {
    #[serde(with = "crate::base64_bytes")]
    pub data: Vec<u8>,
    pub mime_type: String,
}

impl BinaryUpload {
    pub fn new(data: Vec<u8>, mime_type: impl Into<String>) -> Self {
        Self { data, mime_type: mime_type.into() }
    }
}

