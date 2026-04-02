//! Smart daemon discovery.
//!
//! - If a daemon is already running on the well-known port, connect to it.
//! - Otherwise, start an embedded daemon + WS server and become the host.

use std::sync::Arc;

use crate::api::DaemonApi;
use crate::embedded::EmbeddedDaemon;
use crate::storage::SqliteStores;

use super::client::RemoteDaemon;
use super::server;

const DEFAULT_DAEMON_PORT: u16 = 9800;

/// Result of daemon discovery.
pub enum DaemonHandle {
    /// This process is hosting the daemon. Dropping shuts down the WS server.
    Host {
        daemon: Arc<dyn DaemonApi>,
        _server: server::ServerHandle,
    },
    /// Connected to a remote daemon.
    Remote {
        daemon: Arc<dyn DaemonApi>,
    },
}

impl DaemonHandle {
    pub fn daemon(&self) -> Arc<dyn DaemonApi> {
        match self {
            DaemonHandle::Host { daemon, .. } => Arc::clone(daemon),
            DaemonHandle::Remote { daemon, .. } => Arc::clone(daemon),
        }
    }

    pub fn is_host(&self) -> bool {
        matches!(self, DaemonHandle::Host { .. })
    }
}

/// Try to connect to an existing daemon. If none is running, start one.
pub async fn connect_or_host(port: Option<u16>) -> anyhow::Result<DaemonHandle> {
    let port = port.unwrap_or(DEFAULT_DAEMON_PORT);
    let addr = format!("127.0.0.1:{}", port);

    // Try to connect with a short timeout — fail fast if nothing is listening
    let connect_result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        RemoteDaemon::connect(&addr),
    )
    .await;

    match connect_result {
        Ok(Ok(remote)) => {
            tracing::info!(port, "Connected to existing daemon");
            return Ok(DaemonHandle::Remote {
                daemon: remote as Arc<dyn DaemonApi>,
            });
        }
        Ok(Err(e)) => {
            tracing::debug!(port, error = %e, "No daemon at port, will start embedded");
        }
        Err(_) => {
            tracing::debug!(port, "Connect timed out, will start embedded");
        }
    }

    // No daemon running — start one
    tracing::info!(port, "Starting embedded daemon + WS server");

    config::load_env_file();
    let stores = Arc::new(SqliteStores::open()?);
    let daemon = EmbeddedDaemon::new(stores).await?;
    let daemon: Arc<dyn DaemonApi> = daemon;

    let server_handle = server::start(Arc::clone(&daemon), port).await?;

    Ok(DaemonHandle::Host {
        daemon,
        _server: server_handle,
    })
}
