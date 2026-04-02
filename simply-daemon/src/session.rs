//! Client-side session wrapper.
//!
//! Provides a higher-level API around the raw `SessionApi` trait methods,
//! bundling the session ID, event stream, and daemon reference.

use std::sync::Arc;

use tokio::sync::broadcast;

use crate::api::*;

/// A client-side session handle. Wraps create/send/close lifecycle.
///
/// ```ignore
/// let session = DaemonSession::create(daemon, opts).await?;
/// session.send(UserMessage { content: vec![...], tool_filter: None }).await?;
/// while let Ok(event) = session.events().recv().await {
///     match event {
///         DaemonEvent::TextDelta(t) => { /* ... */ }
///         DaemonEvent::TurnComplete => break,
///         _ => {}
///     }
/// }
/// // session is closed on drop (or call session.close().await)
/// ```
pub struct DaemonSession {
    daemon: Arc<dyn DaemonApi>,
    pub info: SessionInfo,
    events: broadcast::Receiver<DaemonEvent>,
    closed: bool,
}

impl DaemonSession {
    /// Create a new session.
    pub async fn create(
        daemon: Arc<dyn DaemonApi>,
        options: CreateSessionOptions,
    ) -> anyhow::Result<Self> {
        let (info, events) = daemon.create_session(options).await?;
        Ok(Self { daemon, info, events, closed: false })
    }

    /// Resume an existing session by session ID.
    pub async fn resume(
        daemon: Arc<dyn DaemonApi>,
        session_id: &SessionId,
    ) -> anyhow::Result<Self> {
        let (info, events) = daemon.resume_session(session_id).await?;
        Ok(Self { daemon, info, events, closed: false })
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
        self.daemon.send_message(&self.info.id, message).await
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
        self.daemon.close_session(&self.info.id).await
    }
}

impl Drop for DaemonSession {
    fn drop(&mut self) {
        if !self.closed {
            let daemon = Arc::clone(&self.daemon);
            let session_id = self.info.id.clone();
            tokio::spawn(async move {
                let _ = daemon.close_session(&session_id).await;
            });
        }
    }
}
