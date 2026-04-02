//! WebSocket client/server for remote daemon access.
//!
//! - [`server`] — wraps a `DaemonApi` and serves it over WebSocket
//! - [`client`] — `RemoteDaemon` implements `DaemonApi` over a WS connection
//! - [`discovery`] — `connect_or_host()` for smart daemon discovery

pub mod protocol;
pub mod server;
pub mod client;
pub mod discovery;

pub use client::RemoteDaemon;
pub use discovery::{connect_or_host, DaemonHandle};
pub use server::ServerHandle;
