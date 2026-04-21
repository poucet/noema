//! Skill system — pluggable capabilities that extend the daemon with tools.
//!
//! A skill is a bidirectional plugin:
//! - **Daemon → Skill**: daemon routes tool calls to the skill
//! - **Skill → Daemon**: skill calls daemon APIs via `Arc<dyn Daemon>`
//!
//! # Auth flow
//! When calling a skill's tool, the daemon forwards the same
//! `simply_rpc::RequestContext` that carried the call through the RPC
//! layer — it already has user identity, OAuth tokens, and free-form
//! origin metadata. The LLM never sees auth tokens.
//!
//! # Deployment modes
//! - **Embedded**: skill lives in daemon process, `Arc<EmbeddedDaemon>`
//! - **Remote**: skill lives in another process (e.g., Lumina), wrapped
//!   as MCP server. Daemon sends auth context as Bearer header on MCP calls.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use llm::ToolDefinition;
use rmcp::model::CallToolResult;
use simply_rpc::RequestContext;

use crate::Daemon;

/// OAuth provider requirement for a skill — the daemon handles the auth flow.
///
/// `provider_id` is the shared provider name (e.g., "google", "notion").
/// Multiple skills and MCP servers can reference the same provider — tokens
/// are stored per `(user_id, provider_id)` so authenticating once covers all.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OAuthRequirement {
    /// OAuth provider ID — shared across all consumers of the same provider.
    /// e.g., "google" for all Google-based skills and MCP servers.
    /// Used as the key in `TransientTokenStore` and `RequestContext.tokens`.
    pub provider_id: String,
    /// Human-readable name (e.g., "Google").
    pub display_name: String,
    /// Authorization endpoint URL.
    pub authorization_url: String,
    /// Token endpoint URL.
    pub token_url: String,
    /// Required scopes (merged with any already-registered scopes for this provider).
    pub scopes: Vec<String>,
    /// Userinfo endpoint (for resolving user email after auth).
    pub userinfo_url: Option<String>,
}

/// A pluggable skill that provides tools to the daemon.
///
/// # Construction
/// Skills receive `Arc<dyn Daemon>` at construction — their handle to
/// the daemon's APIs. Works for both embedded and remote daemons.
///
/// # Tool calls
/// `call_tool` receives the same `RequestContext` that flowed through the
/// RPC layer — user id, OAuth tokens, and origin metadata.
#[async_trait]
pub trait Skill: Send + Sync {
    /// Unique name for this skill (e.g., "gdocs", "github").
    fn name(&self) -> &str;

    /// Tool definitions provided by this skill.
    fn tools(&self) -> Vec<ToolDefinition>;

    /// OAuth providers this skill needs. The daemon registers auth routes
    /// for each and populates tokens in RequestContext.
    /// Default: no OAuth needed.
    fn oauth_requirements(&self) -> Vec<OAuthRequirement> { vec![] }

    /// Execute a tool by name.
    ///
    /// `ctx` is the caller's RequestContext — user identity, OAuth tokens,
    /// and free-form origin metadata. Returns an `rmcp::CallToolResult` so
    /// the response serializes identically whether the skill is embedded or
    /// running remotely.
    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
        ctx: &RequestContext,
    ) -> Result<CallToolResult>;
}

/// Factory function type for constructing a skill with a daemon handle.
/// Used by hosts (daemon main.rs, Lumina) to create skills at startup.
pub type SkillFactory = Box<dyn FnOnce(Arc<dyn Daemon>) -> Box<dyn Skill> + Send>;
