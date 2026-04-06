//! Simply Daemon
//!
//! The daemon is the hub of the Simply platform. It owns storage, session
//! management, MCP tool registry, and agent orchestration.
//!
//! Access is through the [`Daemon`] trait, which has two implementations:
//! - **In-process** ([`embedded::EmbeddedDaemon`]) — linked directly into the
//!   host binary (Noema, Lumina, tests). No networking.
//! - **Remote** ([`remote::RemoteDaemon`]) — calls go over WebSocket to a
//!   standalone daemon process.

#[macro_use] pub mod api;
pub mod admin;
pub mod embedded;
pub mod mcp;
pub mod oauth;
pub mod remote;
pub mod services;
pub mod session;
pub mod storage;
pub mod net;
pub mod tools;
pub mod ws_dispatch;

/// Re-exported types for clients.
pub use api::types as types;
pub use simply_core::ToolService;
pub use remote::RemoteDaemon;
pub use session::DaemonSession;
pub use net::ConnectionState;
