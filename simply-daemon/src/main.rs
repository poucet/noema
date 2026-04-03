//! Simply Daemon — standalone runner.
//!
//! Hosts the daemon as a separate process. Single port for REST + admin.
//! WebSocket server on a separate port for streaming (to be merged).

use std::sync::Arc;

use simply_daemon::api::*;
use simply_daemon::embedded::EmbeddedDaemon;
use simply_daemon::storage::SqliteStores;
use simply_daemon::net;
use simply_rpc::{RestDispatcher, RpcService};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Log to file if DAEMON_LOG_FILE is set, otherwise stderr
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "simply_daemon=info,simply_core=info".into());

    if let Ok(log_path) = std::env::var("DAEMON_LOG_FILE") {
        let file = std::fs::File::create(&log_path)?;
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(file)
            .with_ansi(false)
            .init();
        eprintln!("Logging to {log_path}");
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .init();
    };

    tracing::info!("simply-daemon starting");

    config::load_env_file();
    let settings = config::Settings::load();
    let port = settings.daemon_port.unwrap_or(9800);

    // Open storage and create daemon
    let stores = Arc::new(SqliteStores::open()?);
    let daemon = EmbeddedDaemon::new(stores).await?;
    let daemon_info_svc = <dyn DaemonInfoApi>::service(daemon.clone() as Arc<dyn DaemonInfoApi>);
    let daemon: Arc<dyn DaemonApi> = daemon;

    // WS server — streaming sessions (still separate port, WS upgrade coming to main server)
    let ws_dispatch = build_ws_dispatch(Arc::clone(&daemon));
    let _ws_server = net::server::start(ws_dispatch.clone(), port).await?;
    let tracker = _ws_server.tracker().clone();

    // Kill channel
    let (kill_tx, mut kill_rx) = mpsc::channel(1);

    // Main server (axum) — REST + admin (+ WS upgrade in future)
    let rest_port = port + 1;
    let rest_dispatcher = RestDispatcher::new()
        .register(<dyn SessionApi>::service(daemon.clone()))
        .register(<dyn ConversationApi>::service(daemon.clone()))
        .register(<dyn AssetApi>::service(daemon.clone()))
        .register(<dyn McpApi>::service(daemon.clone()))
        .register(<dyn OAuthApi>::service(daemon.clone()))
        .register(<dyn ModelApi>::service(daemon.clone()))
        .register(<dyn VoiceApi>::service(daemon.clone()))
        .register(daemon_info_svc);

    let _rest_server = net::rest::start(net::rest::ServerConfig {
        rest_dispatcher,
        ws_dispatch: Some(ws_dispatch),
        port: rest_port,
        tracker,
        kill_tx: kill_tx.clone(),
    }).await?;

    tracing::info!(ws_port = port, rest_port = rest_port, "daemon ready");

    // Wait for shutdown signal (Ctrl+C, SIGTERM, or /kill REST endpoint)
    tokio::select! {
        _ = shutdown_signal() => {}
        _ = kill_rx.recv() => { tracing::info!("kill received via REST"); }
    }

    tracing::info!("shutting down");
    daemon.close_all_sessions().await?;
    tracing::info!("shutdown complete");

    Ok(())
}

/// Build the WS dispatch function — streaming sessions only.
///
/// All non-streaming methods use REST. WS only handles SessionApi stream methods.
fn build_ws_dispatch(daemon: Arc<dyn DaemonApi>) -> net::server::DispatchFn {
    let session_svc = <dyn SessionApi>::service(daemon.clone());

    Arc::new(move |method: String, params: serde_json::Value, write_tx: mpsc::Sender<String>| {
        let session_svc = session_svc.clone();

        Box::pin(async move {
            // SessionApi (stream-producing)
            if let Some(dr) = session_svc.dispatch(&method, params.clone()).await {
                for rx in dr.streams {
                    let sid = extract_session_id(&method, &dr.result, &params);
                    spawn_event_forwarder(&sid, rx, write_tx.clone());
                }
                return to_ws_response(dr.result);
            }

            // Unknown method on WS — REST methods should go through HTTP
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

fn to_ws_response(result: simply_rpc::RpcResult) -> net::protocol::WsResponse {
    match result {
        Ok(v) => net::protocol::WsResponse { id: 0, result: Some(v), error: None },
        Err(e) => net::protocol::WsResponse::err(0, e),
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
                    let notif = net::protocol::WsNotification {
                        method: "session.event".to_string(),
                        params: serde_json::to_value(net::protocol::SessionEventParams {
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
