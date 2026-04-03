//! WebSocket protocol types — re-exports generic types from simply-rpc,
//! plus daemon-specific event payload.

pub use simply_rpc::protocol::*;

use serde::{Deserialize, Serialize};
use crate::api::types::{DaemonEvent, SessionId};

/// Session event notification payload (daemon-specific).
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionEventParams {
    pub session_id: SessionId,
    pub event: DaemonEvent,
}
