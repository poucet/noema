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

// ---------------------------------------------------------------------------
// Content-aware dispatch
// ---------------------------------------------------------------------------

/// A typed content part from an RPC method result.
///
/// The `#[rpc_service]` macro generates dispatch that produces these from
/// concrete return types — `BinaryResponse` → `Binary`, everything else → `Json`.
/// Consumers (e.g. MCP tool services) map these to their domain types without
/// duck-typing or runtime metadata checks.
#[derive(Debug, Clone)]
pub enum ContentPart {
    /// Serialized JSON from a regular return type.
    Json(serde_json::Value),
    /// Binary data with mime type (from `BinaryResponse` return types).
    Binary { data: Vec<u8>, mime_type: String },
}

/// Convert an RPC return type into content parts.
///
/// The macro-generated `rest_dispatch_as_content` calls this on the concrete
/// return type of every method. Implement for types that need non-JSON
/// representation (e.g. `BinaryResponse` → binary content).
///
/// For standard serializable types, use the blanket-friendly `impl_json_content!`
/// macro or implement manually.
pub trait IntoContent {
    fn into_content(self) -> Vec<ContentPart>;
}

impl IntoContent for BinaryResponse {
    fn into_content(self) -> Vec<ContentPart> {
        vec![ContentPart::Binary {
            data: self.data,
            mime_type: self.mime_type,
        }]
    }
}

/// Implement `IntoContent` as JSON serialization for a type.
#[macro_export]
macro_rules! impl_json_content {
    ($($ty:ty),* $(,)?) => {
        $(
            impl $crate::IntoContent for $ty {
                fn into_content(self) -> Vec<$crate::ContentPart> {
                    let v = ::serde_json::to_value(&self).unwrap_or_default();
                    vec![$crate::ContentPart::Json(v)]
                }
            }
        )*
    };
}

// Common types that serialize to JSON content.
impl_json_content!(
    serde_json::Value,
    String,
    bool,
    u8, u16, u32, u64, usize,
    i8, i16, i32, i64, isize,
    f32, f64,
);

impl IntoContent for () {
    fn into_content(self) -> Vec<ContentPart> {
        vec![]
    }
}

impl<T: Serialize> IntoContent for Vec<T> {
    fn into_content(self) -> Vec<ContentPart> {
        let v = serde_json::to_value(&self).unwrap_or_default();
        vec![ContentPart::Json(v)]
    }
}
