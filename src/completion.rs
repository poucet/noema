use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::error::CompletionError;

/// A completion suggestion with optional typed metadata
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Completion<M = ()> {
    /// The actual value to insert
    pub value: String,

    /// Optional human-readable label (defaults to value if None)
    pub label: Option<String>,

    /// Optional description for tooltips/help text
    pub description: Option<String>,

    /// Optional typed metadata for UI-specific rendering
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<M>,
}

impl<M> Completion<M> {
    /// Create a simple completion with just a value
    pub fn simple(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: None,
            description: None,
            metadata: None,
        }
    }

    /// Create a completion with a description
    pub fn with_description(value: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: None,
            description: Some(description.into()),
            metadata: None,
        }
    }

    /// Add metadata to this completion
    pub fn with_metadata(mut self, metadata: M) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Add a custom label
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// Trait for types that can provide async completions
/// Type parameter T is the target type (defaults to () for context-free completion)
#[async_trait]
pub trait AsyncCompleter<T = ()>: Send + Sync {
    /// Generate completions from the completion context
    /// Use context.partial() to get the word being completed
    async fn complete<'a>(
        &self,
        context: &Context<'a, T>,
    ) -> Result<Vec<Completion>, CompletionError>;
}
