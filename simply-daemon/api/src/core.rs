//! Daemon-level operations: health, version, shutdown.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Health check response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonHealth {
    pub status: String,
}

#[simply_rpc::rpc_service("daemon")]
#[async_trait]
pub trait CoreApi: Send + Sync {
    /// Check daemon health.
    #[rpc(get = "/daemon", no_tool)]
    async fn health(&self) -> anyhow::Result<DaemonHealth>;

    /// Shut down the daemon.
    #[rpc(post = "/daemon/kill", no_tool)]
    async fn kill(&self) -> anyhow::Result<()>;

    /// Get daemon version.
    #[rpc(get = "/daemon/version", no_tool)]
    async fn version(&self) -> anyhow::Result<String>;

    /// Get the daemon's public URL (for OAuth callbacks, auth links, etc.).
    #[rpc(get = "/daemon/public-url", no_tool)]
    async fn public_url(&self) -> anyhow::Result<String>;
}
