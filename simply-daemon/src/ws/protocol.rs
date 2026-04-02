//! WebSocket JSON-RPC-like protocol types.

use serde::{Deserialize, Serialize};

use crate::api::types::{DaemonEvent, SessionId};

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

    pub fn err(id: u64, message: impl Into<String>) -> Self {
        Self {
            id,
            result: None,
            error: Some(WsError {
                code: -1,
                message: message.into(),
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

/// Session event notification payload.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionEventParams {
    pub session_id: SessionId,
    pub event: DaemonEvent,
}

/// Incoming WS text frame — could be request, response, or notification.
/// Parsed by checking which fields are present.
#[derive(Debug, Deserialize)]
pub struct WsIncoming {
    /// Present on requests and responses.
    pub id: Option<u64>,
    /// Present on requests and notifications.
    pub method: Option<String>,
    /// Present on successful responses.
    pub result: Option<serde_json::Value>,
    /// Present on error responses.
    pub error: Option<WsError>,
    /// Present on requests and notifications.
    #[serde(default)]
    pub params: serde_json::Value,
}

impl WsIncoming {
    /// Is this a request? (has id + method)
    pub fn is_request(&self) -> bool {
        self.id.is_some() && self.method.is_some()
    }

    /// Is this a response? (has id, no method)
    pub fn is_response(&self) -> bool {
        self.id.is_some() && self.method.is_none()
    }

    /// Is this a notification? (has method, no id)
    pub fn is_notification(&self) -> bool {
        self.id.is_none() && self.method.is_some()
    }
}
