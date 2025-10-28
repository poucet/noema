use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::CompletionError;
use crate::token_stream::TokenStream;

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
pub struct CompletionContext<'a, T> {
    /// Parsed token stream (contains input, cursor, and tokens)
    pub tokens: TokenStream,

    /// Reference to the target for context-aware completion
    pub target: &'a T,
}

impl<'a, T> CompletionContext<'a, T> {
    /// Create a new completion context
    pub fn new(input: String, cursor: usize, target: &'a T) -> Self {
        let tokens = TokenStream::new(input, cursor);

        Self {
            tokens,
            target,
        }
    }

    /// Calculate which argument index is being completed (delegates to TokenStream)
    pub fn arg_index(&self) -> usize {
        self.tokens.arg_index()
    }

    /// Get the partial word being completed (delegates to TokenStream)
    pub fn partial(&self) -> &str {
        self.tokens.partial()
    }

    /// Get the input string (delegates to TokenStream)
    pub fn input(&self) -> &str {
        self.tokens.input()
    }
}

/// Trait for types that can provide async completions
/// Type parameter T is the target type (defaults to () for context-free completion)
#[async_trait]
pub trait AsyncCompleter<T = ()>: Send + Sync {
    /// Generate completions for the given partial input with access to target
    async fn complete(
        &self,
        partial: &str,
        context: &CompletionContext<T>,
    ) -> Result<Vec<Completion>, CompletionError>;
}
