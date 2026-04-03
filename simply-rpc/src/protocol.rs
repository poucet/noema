//! WebSocket JSON-RPC-like protocol types.
//!
//! These are the wire-format types for WebSocket RPC communication.
//! Generic — no knowledge of specific APIs or daemon types.

use serde::{Deserialize, Serialize};

/// Client → Server request.
#[derive(Debug, Serialize, Deserialize)]
pub struct WsRequest {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Server → Client response.
#[derive(Debug, Serialize, Deserialize)]
pub struct WsResponse {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<WsError>,
}

impl WsResponse {
    pub fn ok(id: u64, result: impl Serialize) -> Self {
        Self {
            id,
            result: Some(serde_json::to_value(result).unwrap_or_default()),
            error: None,
        }
    }

    pub fn err(id: u64, message: impl std::fmt::Display) -> Self {
        Self {
            id,
            result: None,
            error: Some(WsError {
                code: -1,
                message: message.to_string(),
            }),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WsError {
    pub code: i32,
    pub message: String,
}

/// Server → Client push notification (no id).
#[derive(Debug, Serialize, Deserialize)]
pub struct WsNotification {
    pub method: String,
    pub params: serde_json::Value,
}

/// Incoming WS text frame — could be request, response, or notification.
/// Parsed by checking which fields are present.
#[derive(Debug, Deserialize)]
pub struct WsIncoming {
    pub id: Option<u64>,
    pub method: Option<String>,
    pub result: Option<serde_json::Value>,
    pub error: Option<WsError>,
    #[serde(default)]
    pub params: serde_json::Value,
}

impl WsIncoming {
    pub fn is_request(&self) -> bool {
        self.id.is_some() && self.method.is_some()
    }

    pub fn is_response(&self) -> bool {
        self.id.is_some() && self.method.is_none()
    }

    pub fn is_notification(&self) -> bool {
        self.id.is_none() && self.method.is_some()
    }
}
