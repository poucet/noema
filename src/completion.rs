use async_trait::async_trait;
use serde::{Deserialize, Serialize};

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

/// Context provided to completers during completion
#[derive(Clone, Debug)]
pub struct CompletionContext {
    /// Full input string
    pub input: String,

    /// Cursor position in input
    pub cursor: usize,

    /// Parsed tokens (for convenience)
    pub tokens: Vec<String>,
}

impl CompletionContext {
    /// Create a new completion context
    pub fn new(input: String, cursor: usize) -> Self {
        // Simple whitespace tokenization
        // TODO: Handle quotes properly
        let tokens = input.split_whitespace().map(String::from).collect();

        Self {
            input,
            cursor,
            tokens,
        }
    }
}

/// Trait for types that can provide async completions
#[async_trait]
pub trait AsyncCompleter: Send + Sync {
    /// Metadata type for completions
    type Metadata: Serialize + Send + Sync + Clone;

    /// Generate completions for the given partial input
    async fn complete(
        &self,
        partial: &str,
        context: &CompletionContext,
    ) -> Result<Vec<Completion<Self::Metadata>>, CompletionError>;
}
