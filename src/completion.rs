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

/// Parsed tokens with methods for consumption
#[derive(Clone, Debug)]
pub struct TokenStream {
    tokens: Vec<String>,
}

impl TokenStream {
    /// Create TokenStream with simple whitespace tokenization (for completion)
    pub fn new(input: &str) -> Self {
        let tokens = input.split_whitespace().map(String::from).collect();
        Self { tokens }
    }

    /// Create TokenStream respecting quotes (for command argument parsing)
    pub fn from_quoted(input: &str) -> Self {
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '"' => {
                    in_quotes = !in_quotes;
                }
                ' ' | '\t' if !in_quotes => {
                    if !current.is_empty() {
                        tokens.push(current.clone());
                        current.clear();
                    }
                }
                '\\' if in_quotes => {
                    // Handle escape sequences in quotes
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                }
                _ => {
                    current.push(ch);
                }
            }
        }

        if !current.is_empty() {
            tokens.push(current);
        }

        Self { tokens }
    }

    /// Get token at index
    pub fn get(&self, index: usize) -> Option<&str> {
        self.tokens.get(index).map(|s| s.as_str())
    }

    /// Get the last token (what's being completed)
    pub fn last(&self) -> Option<&str> {
        self.tokens.last().map(|s| s.as_str())
    }

    /// Number of tokens
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// Parse token at index as type T
    pub fn parse<T: std::str::FromStr>(&self, index: usize) -> Option<T> {
        self.get(index).and_then(|s| s.parse().ok())
    }
}

/// Context provided to completers during completion
pub struct CompletionContext<'a, T> {
    /// Full input string
    pub input: String,

    /// Cursor position in input
    pub cursor: usize,

    /// Parsed tokens
    pub tokens: TokenStream,

    /// Reference to the target for context-aware completion
    pub target: &'a T,
}

impl<'a, T> CompletionContext<'a, T> {
    /// Create a new completion context
    pub fn new(input: String, cursor: usize, target: &'a T) -> Self {
        let tokens = TokenStream::new(&input);

        Self {
            input,
            cursor,
            tokens,
            target,
        }
    }

    /// Calculate which argument index is being completed
    pub fn arg_index(&self) -> usize {
        if self.input.ends_with(char::is_whitespace) {
            self.tokens.len().saturating_sub(1)
        } else {
            self.tokens.len().saturating_sub(2)
        }
    }

    /// Get the partial word being completed
    pub fn partial(&self) -> &str {
        if self.input.ends_with(char::is_whitespace) {
            ""  // Completing a new word
        } else {
            self.tokens.last().unwrap_or("")
        }
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
