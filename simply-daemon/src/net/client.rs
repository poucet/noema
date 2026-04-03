//! Daemon-specific WebSocket client — thin wrapper over simply-rpc's generic WsConnection.

use std::sync::Arc;

use simply_rpc::ws_client::{NotificationDemux, WsConnection};

use crate::api::types::DaemonEvent;
use crate::net::protocol::SessionEventParams;

/// Type alias for the daemon's WsConnection with DaemonEvent streams.
pub type DaemonWsConnection = WsConnection<DaemonEvent>;

/// Create a daemon-specific notification demuxer.
///
/// Parses "session.event" notifications into (session_id, DaemonEvent).
pub fn daemon_demux() -> NotificationDemux<DaemonEvent> {
    Arc::new(|method, params| {
        if method == "session.event" {
            if let Ok(p) = serde_json::from_value::<SessionEventParams>(params) {
                return Some((p.session_id.as_str().to_string(), p.event));
            }
        }
        None
    })
}

/// Connect to a daemon with automatic reconnection.
pub async fn connect(addr: &str) -> anyhow::Result<DaemonWsConnection> {
    WsConnection::connect(addr, daemon_demux()).await
}
