//! Request context — per-request metadata flowing through the service layer.

use serde::{Deserialize, Serialize};

use crate::Scope;

/// Per-request context carrying scope and request-level metadata.
///
/// Every API method receives this as the first parameter. The RPC macro
/// handles serialization/deserialization transparently — it's injected
/// by the dispatch layer, not deserialized from user-provided params.
///
/// `metadata` is a free-form map for origin/context hints (e.g.
/// `"discord.guild_id"`, `"discord.channel_id"`) that callers can stash
/// when creating a session so skill handlers can act on "where the
/// conversation is happening" without the LLM having to pass it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RequestContext {
    /// User identity and authorization scope.
    pub scope: Scope,
    /// OAuth tokens for this user, keyed by provider ID.
    /// Populated by the daemon from its token store before tool dispatch.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub tokens: std::collections::HashMap<String, String>,
    /// Free-form origin/context metadata (e.g. `discord.guild_id`,
    /// `discord.channel_id`). Skills read these to act on the ambient
    /// conversation context.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl RequestContext {
    pub fn anonymous() -> Self {
        Self {
            scope: Scope::anonymous(),
            tokens: Default::default(),
            metadata: Default::default(),
        }
    }

    pub fn with_scope(scope: Scope) -> Self {
        Self { scope, tokens: Default::default(), metadata: Default::default() }
    }

    pub fn with_token(mut self, provider_id: impl Into<String>, token: impl Into<String>) -> Self {
        self.tokens.insert(provider_id.into(), token.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Read a metadata value as a string (if present and string-typed).
    pub fn metadata_str(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).and_then(|v| v.as_str())
    }

    /// Read a metadata value as u64 (accepts JSON number or stringified number).
    pub fn metadata_u64(&self, key: &str) -> Option<u64> {
        let v = self.metadata.get(key)?;
        v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    }
}

impl From<Scope> for RequestContext {
    fn from(scope: Scope) -> Self {
        Self { scope, tokens: Default::default(), metadata: Default::default() }
    }
}
