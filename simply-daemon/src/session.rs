//! Client-side session wrapper.
//!
//! Provides a higher-level API around the raw `SessionApi` trait methods,
//! bundling the session ID, event stream, and daemon reference.

use std::sync::Arc;

use simply_rpc::RequestContext;
use tokio::sync::broadcast;

use crate::api::*;

/// A client-side session handle. Wraps create/send/close lifecycle.
pub struct DaemonSession {
    daemon: Arc<dyn Daemon>,
    ctx: RequestContext,
    pub info: SessionInfo,
    events: broadcast::Receiver<DaemonEvent>,
    closed: bool,
}

impl DaemonSession {
    /// Create a new session with a request context.
    pub async fn create(
        daemon: Arc<dyn Daemon>,
        ctx: RequestContext,
        options: CreateSessionOptions,
    ) -> anyhow::Result<Self> {
        let (info, events) = daemon.session().create_session(&ctx, options).await?;
        Ok(Self { daemon, ctx, info, events, closed: false })
    }

    /// Session ID.
    pub fn id(&self) -> &SessionId {
        &self.info.id
    }

    /// Model ID for this session.
    pub fn model_id(&self) -> &str {
        &self.info.model_id
    }

    /// Send a user message. Events arrive via `recv()`.
    pub async fn send(&self, message: UserMessage) -> anyhow::Result<()> {
        self.daemon.session().send_message(&self.ctx, &self.info.id, message).await
    }

    /// Receive the next event from this session's stream.
    pub async fn recv(&mut self) -> Result<DaemonEvent, broadcast::error::RecvError> {
        self.events.recv().await
    }

    /// Get a mutable reference to the underlying event receiver.
    pub fn events(&mut self) -> &mut broadcast::Receiver<DaemonEvent> {
        &mut self.events
    }

    /// Explicitly close the session.
    pub async fn close(mut self) -> anyhow::Result<()> {
        self.closed = true;
        self.daemon.session().close_session(&self.ctx, &self.info.id).await
    }
}

impl Drop for DaemonSession {
    fn drop(&mut self) {
        if !self.closed {
            let daemon = Arc::clone(&self.daemon);
            let ctx = self.ctx.clone();
            let session_id = self.info.id.clone();
            tokio::spawn(async move {
                let _ = daemon.session().close_session(&ctx, &session_id).await;
            });
        }
    }
}
