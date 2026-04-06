//! Shared WebSocket dispatch builder for session streaming.
//!
//! Used by both the standalone daemon (`main.rs`) and embedded daemon (`connect_or_host`).

use std::sync::Arc;

use simply_rpc::RpcService;
use tokio::sync::mpsc;

use crate::api::*;
use crate::net::{protocol, server};

/// Build a WS dispatch function that handles session streaming.
pub fn build(session_api: Arc<dyn SessionApi>) -> server::DispatchFn {
    let session_svc = <dyn SessionApi>::service(session_api);

    Arc::new(move |method: String, params: serde_json::Value, write_tx: mpsc::Sender<String>| {
        let session_svc = session_svc.clone();

        Box::pin(async move {
            if let Some(dr) = session_svc.dispatch(&method, params.clone()).await {
                for rx in dr.streams {
                    let sid = extract_session_id(&method, &dr.result, &params);
                    spawn_event_forwarder(&sid, rx, write_tx.clone());
                }
                return to_ws_response(dr.result);
            }

            to_ws_response(Err(anyhow::anyhow!("unknown WS method: {method} — use REST for non-streaming methods")))
        })
    })
}

fn extract_session_id(
    method: &str,
    result: &simply_rpc::RpcResult,
    params: &serde_json::Value,
) -> SessionId {
    if method.ends_with("subscribe_session") {
        serde_json::from_value(params.clone()).unwrap_or_else(|_| SessionId::new("unknown"))
    } else {
        result
            .as_ref()
            .ok()
            .and_then(|v| serde_json::from_value::<SessionInfo>(v.clone()).ok())
            .map(|info| info.id)
            .unwrap_or_else(|| SessionId::new("unknown"))
    }
}

fn to_ws_response(result: simply_rpc::RpcResult) -> protocol::WsResponse {
    match result {
        Ok(v) => protocol::WsResponse { id: 0, result: Some(v), error: None },
        Err(e) => protocol::WsResponse::err(0, e),
    }
}

fn spawn_event_forwarder(
    session_id: &SessionId,
    mut rx: tokio::sync::broadcast::Receiver<DaemonEvent>,
    write_tx: mpsc::Sender<String>,
) {
    let sid = session_id.clone();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let notif = protocol::WsNotification {
                        method: "session.event".to_string(),
                        params: serde_json::to_value(protocol::SessionEventParams {
                            session_id: sid.clone(), event,
                        }).unwrap_or_default(),
                    };
                    if write_tx.send(serde_json::to_string(&notif).unwrap_or_default()).await.is_err() { break; }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(session_id = %sid, skipped = n, "event forwarder lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}
