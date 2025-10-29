use crate::token_stream::TokenStream;

/// Context for command completion (immutable target reference)
pub struct Context<'a, T> {
    /// Parsed token stream (contains input, cursor, and tokens)
    pub tokens: TokenStream,

    /// Immutable reference to the target
    pub target: &'a T,
}

impl<'a, T> Context<'a, T> {
    /// Create a new context with stringable input
    pub fn new(input: impl ToString, target: &'a T) -> Self {
        let input_string = input.to_string();
        let tokens = TokenStream::new(input_string);
        Self { tokens, target }
    }

    /// Create context from TokenStream (for advanced use cases)
    pub fn from_tokens(tokens: TokenStream, target: &'a T) -> Self {
        Self { tokens, target }
    }

    /// Get the input string
    pub fn stream(&self) -> &TokenStream {
        &self.tokens
    }
}

/// Context for command execution (mutable target reference)
pub struct ContextMut<'a, T> {
    /// Parsed token stream (contains parsed arguments)
    pub tokens: TokenStream,

    /// Mutable reference to the target
    pub target: &'a mut T,
}

impl<'a, T> ContextMut<'a, T> {
    /// Create a new mutable context
    pub fn new(tokens: TokenStream, target: &'a mut T) -> Self {
        Self { tokens, target }
    }

    /// Get the input string token stream
    pub fn stream(&self) -> &TokenStream {
        &self.tokens
    }
}
