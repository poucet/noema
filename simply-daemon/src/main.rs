//! Simply Daemon — standalone runner.
//!
//! Hosts the daemon as a separate process. Clients connect via WebSocket.

use std::collections::HashMap;
use std::sync::Arc;

use simply_daemon::api::*;
use simply_daemon::embedded::EmbeddedDaemon;
use simply_daemon::storage::SqliteStores;
use simply_daemon::ws;
use simply_rpc::{Dispatcher, RpcService};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "simply_daemon=info,simply_core=info".into()),
        )
        .init();

    tracing::info!("simply-daemon starting");

    config::load_env_file();
    let settings = config::Settings::load();
    let port = settings.daemon_port.unwrap_or(9800);

    // Open storage and create daemon
    let stores = Arc::new(SqliteStores::open()?);
    let daemon = EmbeddedDaemon::new(stores).await?;
    let daemon: Arc<dyn DaemonApi> = daemon;

    // Build dispatch — this is where we wire up which services are exposed
    let dispatch = build_dispatch(Arc::clone(&daemon));

    // Start WebSocket server
    let _ws_server = ws::server::start(dispatch, port).await?;

    tracing::info!(ws_port = port, "daemon ready");

    // Wait for shutdown signal
    shutdown_signal().await;

    tracing::info!("shutting down");
    daemon.close_all_sessions().await?;
    tracing::info!("shutdown complete");

    Ok(())
}

/// Build the dispatch function — wires up all daemon services.
///
/// SessionApi is dispatched separately (produces streams → event forwarders).
/// Everything else goes through a Dispatcher.
fn build_dispatch(daemon: Arc<dyn DaemonApi>) -> ws::server::DispatchFn {
    let session_svc = <dyn SessionApi>::service(daemon.clone());

    let dispatcher = Dispatcher::new()
        .register(<dyn ConversationApi>::service(daemon.clone()))
        .register(<dyn AssetApi>::service(daemon.clone()))
        .register(<dyn McpApi>::service(daemon.clone()))
        .register(<dyn OAuthApi>::service(daemon.clone()))
        .register(<dyn ModelApi>::service(daemon.clone()))
        .register(<dyn VoiceApi>::service(daemon.clone()));

    Arc::new(move |method: String, params: serde_json::Value, write_tx: mpsc::Sender<String>| {
        let session_svc = session_svc.clone();
        let dispatcher = dispatcher.clone();

        Box::pin(async move {
            // SessionApi (stream-producing)
            if let Some(dr) = session_svc.dispatch(&method, params.clone()).await {
                for rx in dr.streams {
                    let sid = extract_session_id(&method, &dr.result, &params);
                    spawn_event_forwarder(&sid, rx, write_tx.clone());
                }
                return to_ws_response(dr.result);
            }

            // Everything else
            let result = dispatcher.dispatch(&method, params).await;
            to_ws_response(result)
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

fn to_ws_response(result: simply_rpc::RpcResult) -> ws::protocol::WsResponse {
    match result {
        Ok(v) => ws::protocol::WsResponse { id: 0, result: Some(v), error: None },
        Err(e) => ws::protocol::WsResponse::err(0, e),
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
                    let notif = ws::protocol::WsNotification {
                        method: "session.event".to_string(),
                        params: serde_json::to_value(ws::protocol::SessionEventParams {
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

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = sigterm.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await.ok();
    }
}
