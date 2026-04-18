//! Request context — per-request metadata flowing through the service layer.

use serde::{Deserialize, Serialize};

use crate::Scope;

/// Per-request context carrying scope and request-level metadata.
///
/// Every API method receives this as the first parameter. The RPC macro
/// handles serialization/deserialization transparently — it's injected
/// by the dispatch layer, not deserialized from user-provided params.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RequestContext {
    /// User identity and authorization scope.
    pub scope: Scope,
    /// OAuth tokens for this user, keyed by provider ID.
    /// Populated by the daemon from its token store before tool dispatch.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub tokens: std::collections::HashMap<String, String>,
}

impl RequestContext {
    pub fn anonymous() -> Self {
        Self {
            scope: Scope::anonymous(),
            tokens: Default::default(),
        }
    }

    pub fn with_scope(scope: Scope) -> Self {
        Self { scope, tokens: Default::default() }
    }

    pub fn with_token(mut self, provider_id: impl Into<String>, token: impl Into<String>) -> Self {
        self.tokens.insert(provider_id.into(), token.into());
        self
    }
}

impl From<Scope> for RequestContext {
    fn from(scope: Scope) -> Self {
        Self { scope, tokens: Default::default() }
    }
}
