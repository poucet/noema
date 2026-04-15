//! Request context — per-request metadata flowing through the service layer.

use serde::{Deserialize, Serialize};

use crate::Scope;

/// Per-request context carrying scope and request-level metadata.
///
/// Every API method receives this as the first parameter. The RPC macro
/// handles serialization/deserialization transparently — it's injected
/// by the dispatch layer, not deserialized from user-provided params.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RequestContext {
    /// User identity and authorization scope.
    pub scope: Scope,
    // future: trace_id, request_id, locale, etc.
}

impl RequestContext {
    pub fn anonymous() -> Self {
        Self {
            scope: Scope::anonymous(),
        }
    }

    pub fn with_scope(scope: Scope) -> Self {
        Self { scope }
    }
}

impl From<Scope> for RequestContext {
    fn from(scope: Scope) -> Self {
        Self { scope }
    }
}
