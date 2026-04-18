//! Core daemon service (health, shutdown, version).

use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::Mutex;
use simply_core::storage::coordinator::StorageCoordinator;
use simply_core::storage::traits::{StorageTypes, Stores};
use simply_rpc::RequestContext;
use crate::api::*;
use tokio::sync::mpsc;

pub struct CoreService {
    kill_tx: Option<tokio::sync::mpsc::Sender<()>>,
}

impl CoreService {
    pub fn new(kill_tx: tokio::sync::mpsc::Sender<()>) -> Self {
        Self { kill_tx: Some(kill_tx) }
    }

    pub fn embedded() -> Self {
        Self { kill_tx: None }
    }
}

#[async_trait]
impl CoreApi for CoreService {
    async fn health(&self) -> anyhow::Result<DaemonHealth> {
        Ok(DaemonHealth { status: "ok".to_string() })
    }

    async fn kill(&self) -> anyhow::Result<()> {
        if let Some(tx) = &self.kill_tx {
            let _ = tx.send(()).await;
        }
        Ok(())
    }

    async fn version(&self) -> anyhow::Result<String> {
        Ok(env!("CARGO_PKG_VERSION").to_string())
    }

    async fn public_url(&self) -> anyhow::Result<String> {
        let settings = config::Settings::load();
        Ok(settings.public_url.unwrap_or_else(|| {
            let port = settings.daemon_port.unwrap_or(config::DEFAULT_DAEMON_PORT);
            format!("http://localhost:{port}")
        }))
    }
}

