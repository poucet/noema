//! Application initialization command

use simply_daemon::api::*;
use simply_daemon::ws;
use simply_daemon::ws::discovery::ServiceBuilders;
use simply_rpc::{Dispatcher, RpcService};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};
use tokio::sync::mpsc;

use crate::logging::log_message;
use crate::state::AppState;

#[tauri::command]
pub async fn init_app(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<String, String> {
    if state.is_initialized() {
        return Ok(String::new());
    }

    // Prevent concurrent init (React StrictMode calls this twice)
    if state.initializing.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Ok(String::new());
    }

    let state_arc = state.inner().clone();
    do_init(app, state_arc).await
}

async fn do_init(_app: AppHandle, state: Arc<AppState>) -> Result<String, String> {
    log_message("Starting app initialization");

    config::load_env_file();
    let settings = config::Settings::load();
    let port = settings.daemon_port;

    let daemon_port = port.unwrap_or(9800);
    let rest_port = daemon_port + 1;

    let handle = ws::connect_or_host(port, service_builders())
        .await
        .map_err(|e| format!("Failed to initialize daemon: {}", e))?;

    let is_host = handle.is_host();
    let daemon = handle.daemon();

    // Set the REST base URL — used by asset protocol handler and other REST clients.
    // When remote, this points to wherever the daemon is running.
    let rest_base_url = format!("http://127.0.0.1:{rest_port}");
    let _ = state.rest_base_url.set(rest_base_url);

    let model_name = daemon.default_model_id().await;
    let _ = state.daemon.set(daemon);
    let _ = state._daemon_handle.set(handle);

    log_message(&format!(
        "Daemon initialized ({}), default model: {}",
        if is_host { "embedded" } else { "remote" },
        model_name
    ));
    Ok(model_name)
}

/// Service wiring — which APIs are exposed over WS and REST.
fn service_builders() -> ServiceBuilders {
    ServiceBuilders {
        ws_dispatch: Box::new(build_ws_dispatch),
        rest_dispatcher: Box::new(|daemon| {
            Dispatcher::new()
                .register(<dyn AssetApi>::service(daemon))
        }),
        client_name: "noema".to_string(),
    }
}

/// Build the WS dispatch function.
fn build_ws_dispatch(daemon: Arc<dyn DaemonApi>) -> ws::server::DispatchFn {
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
            if let Some(dr) = session_svc.dispatch(&method, params.clone()).await {
                for rx in dr.streams {
                    let sid = extract_session_id(&method, &dr.result, &params);
                    spawn_event_forwarder(&sid, rx, write_tx.clone());
                }
                return to_ws_response(dr.result);
            }

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
