//! Skill system — pluggable capabilities that extend the daemon with tools.
//!
//! A skill provides tool definitions and execution logic. Skills access
//! daemon services via API trait objects (`Arc<dyn DocumentApi>`, etc.),
//! getting embedding, validation, and access control for free.
//!
//! Current registration is explicit (daemon constructs at startup).
//! Future: dynamic loading via shared libraries.

use anyhow::Result;
use async_trait::async_trait;
use llm::{ToolDefinition, ToolResultContent};

use crate::types::UserId;

/// A pluggable skill that provides tools to the daemon.
///
/// Skills are constructed by the daemon (or host) at startup, receiving
/// whatever API trait objects they need (`Arc<dyn DocumentApi>`, etc.).
/// The `Skill` trait itself is deliberately minimal — construction is
/// skill-specific.
///
/// # Future: dynamic loading
/// - Skills compiled as cdylib crates
/// - Daemon loads `.so`/`.dylib` from a skills directory
/// - Each library exports a factory function
/// - The `Skill` trait is already object-safe for this purpose
#[async_trait]
pub trait Skill: Send + Sync {
    /// Unique name for this skill (e.g., "gdocs", "github").
    fn name(&self) -> &str;

    /// Tool definitions provided by this skill.
    fn tools(&self) -> Vec<ToolDefinition>;

    /// Execute a tool by name.
    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
        user_id: &UserId,
    ) -> Result<Vec<ToolResultContent>>;
}
