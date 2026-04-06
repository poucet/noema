//! Bidirectional stream handle for RPC stream methods.
//!
//! `StreamHandle<T, U>` represents a bidirectional connection where:
//! - `T` = messages the client sends (serialized as JSON over WS)
//! - `U` = messages the server sends (serialized as JSON over WS)
//!
//! Binary data should use `#[serde(with = "simply_rpc::base64_bytes")]` for
//! base64 encoding in JSON frames.
//!
//! The macro detects `Result<StreamHandle<T, U>>` as the return type and
//! generates WS bridging for both directions.

use tokio::sync::mpsc;

/// A bidirectional stream connection.
///
/// Returned by `#[rpc(stream = "/path")]` methods. The macro bridges
/// this to a WebSocket connection — `T` frames flow client→server,
/// `U` frames flow server→client, both as JSON.
pub struct StreamHandle<T, U> {
    tx: mpsc::Sender<T>,
    rx: mpsc::Receiver<U>,
}

impl<T, U> StreamHandle<T, U> {
    /// Create a new stream handle from raw channels.
    pub fn new(tx: mpsc::Sender<T>, rx: mpsc::Receiver<U>) -> Self {
        Self { tx, rx }
    }

    /// Send a message to the other side.
    pub async fn send(&self, msg: T) -> anyhow::Result<()> {
        self.tx.send(msg).await
            .map_err(|_| anyhow::anyhow!("stream closed"))
    }

    /// Receive a message from the other side.
    pub async fn recv(&mut self) -> Option<U> {
        self.rx.recv().await
    }

    /// Get a reference to the sender.
    pub fn sender(&self) -> &mpsc::Sender<T> {
        &self.tx
    }

    /// Split into the raw sender and receiver.
    pub fn into_parts(self) -> (mpsc::Sender<T>, mpsc::Receiver<U>) {
        (self.tx, self.rx)
    }
}
