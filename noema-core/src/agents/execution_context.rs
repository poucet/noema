//! Execution context for agent tool calls
//!
//! Provides context that agents inject into ALL noema-core tool calls.
//! The context is injected as a `_context` field containing system identifiers.

use crate::storage::ids::{ConversationId, SpanId, TurnId, UserId};
use serde::{Deserialize, Serialize};

/// Execution context injected into tool calls
///
/// Contains identifiers that the system knows but the LLM doesn't.
/// Agents inject this as `_context` into all tool call arguments.
/// Tools that need it (like spawn_agent) deserialize and use it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
}

impl ExecutionContext {
    /// Create a new empty context
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a fully populated context
    pub fn with_all(
        user_id: UserId,
        conversation_id: ConversationId,
        turn_id: TurnId,
        span_id: Option<SpanId>,
        model_id: String,
    ) -> Self {
        Self {
            user_id: Some(user_id.as_str().to_string()),
            conversation_id: Some(conversation_id.as_str().to_string()),
            turn_id: Some(turn_id.as_str().to_string()),
            span_id: span_id.map(|s| s.as_str().to_string()),
            model_id: Some(model_id),
        }
    }

    /// Check if all required fields are set
    pub fn is_ready(&self) -> bool {
        self.user_id.is_some()
            && self.conversation_id.is_some()
            && self.turn_id.is_some()
            && self.model_id.is_some()
    }

    /// Inject this context into tool arguments as `_context` field
    pub fn inject_into(
        &self,
        mut args: serde_json::Map<String, serde_json::Value>,
    ) -> serde_json::Map<String, serde_json::Value> {
        if let Ok(ctx_value) = serde_json::to_value(self) {
            args.insert("_context".into(), ctx_value);
        }
        args
    }
}
