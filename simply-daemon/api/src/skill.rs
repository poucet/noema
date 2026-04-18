//! Skill system — pluggable capabilities that extend the daemon with tools.
//!
//! A skill is a bidirectional plugin:
//! - **Daemon → Skill**: daemon routes tool calls to the skill
//! - **Skill → Daemon**: skill calls daemon APIs via `Arc<dyn Daemon>`
//!
//! # Auth flow
//! When calling a skill's tool, the daemon injects the user's auth context
//! (OAuth tokens, identity) via `SkillCallContext`. The LLM never sees
//! auth tokens — they're resolved by the daemon from its token store.
//!
//! # Deployment modes
//! - **Embedded**: skill lives in daemon process, `Arc<EmbeddedDaemon>`
//! - **Remote**: skill lives in another process (e.g., Lumina), wrapped
//!   as MCP server. Daemon sends auth context as Bearer header on MCP calls.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use llm::{ToolDefinition, ToolResultContent};

use crate::Daemon;
use crate::types::UserId;

/// Context passed to skill tool calls.
///
/// Populated by the daemon before dispatch. Includes the calling user's
/// identity and any OAuth tokens the daemon has for this user.
pub struct SkillCallContext {
    /// The calling user.
    pub user_id: UserId,
    /// OAuth access tokens the daemon has for this user, keyed by provider/server ID.
    /// e.g., {"google-docs": "ya29.a0AfH6SM...", "github": "gho_xxxx"}
    pub tokens: HashMap<String, String>,
}

impl SkillCallContext {
    pub fn new(user_id: UserId) -> Self {
        Self { user_id, tokens: HashMap::new() }
    }

    pub fn with_token(mut self, server_id: impl Into<String>, token: impl Into<String>) -> Self {
        self.tokens.insert(server_id.into(), token.into());
        self
    }

    /// Get an OAuth token for a specific service/server.
    pub fn token(&self, server_id: &str) -> Option<&str> {
        self.tokens.get(server_id).map(|s| s.as_str())
    }
}

/// A pluggable skill that provides tools to the daemon.
///
/// # Construction
/// Skills receive `Arc<dyn Daemon>` at construction — their handle to
/// the daemon's APIs. Works for both embedded and remote daemons.
///
/// # Tool calls
/// Auth tokens for the calling user are passed via `SkillCallContext`.
/// The daemon populates this from its token store before dispatch.
/// The LLM never sees tokens — it just calls `gdocs_import(doc_id: "...")`.
/// OAuth requirement for a skill — the daemon handles the auth flow.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OAuthRequirement {
    /// Unique ID for this auth provider (used as key in token store).
    pub id: String,
    /// Human-readable name (e.g., "Google Docs").
    pub display_name: String,
    /// Authorization endpoint URL.
    pub authorization_url: String,
    /// Token endpoint URL.
    pub token_url: String,
    /// Required scopes.
    pub scopes: Vec<String>,
    /// Userinfo endpoint (for resolving user email after auth).
    pub userinfo_url: Option<String>,
}

#[async_trait]
pub trait Skill: Send + Sync {
    /// Unique name for this skill (e.g., "gdocs", "github").
    fn name(&self) -> &str;

    /// Tool definitions provided by this skill.
    fn tools(&self) -> Vec<ToolDefinition>;

    /// OAuth providers this skill needs. The daemon registers auth routes
    /// for each and populates tokens in SkillCallContext.
    /// Default: no OAuth needed.
    fn oauth_requirements(&self) -> Vec<OAuthRequirement> { vec![] }

    /// Execute a tool by name.
    ///
    /// `ctx` contains the calling user's identity and OAuth tokens.
    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
        ctx: &SkillCallContext,
    ) -> Result<Vec<ToolResultContent>>;
}

/// Factory function type for constructing a skill with a daemon handle.
/// Used by hosts (daemon main.rs, Lumina) to create skills at startup.
pub type SkillFactory = Box<dyn FnOnce(Arc<dyn Daemon>) -> Box<dyn Skill> + Send>;
