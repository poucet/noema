//! Simply Daemon
//!
//! The daemon is the hub of the Simply platform. It owns storage, session
//! management, MCP tool registry, and agent orchestration.
//!
//! Access is through the [`DaemonApi`] trait, which has two implementations:
//! - **In-process** ([`embedded::EmbeddedDaemon`]) — linked directly into the
//!   host binary (Noema, Lumina, tests). No networking.
//! - **Remote** (future) — calls go over WebSocket to a standalone daemon process.

pub mod api;
pub mod embedded;
pub mod mcp;

/// Re-exported types for clients.
pub use api::types as types;