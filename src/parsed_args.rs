use std::str::FromStr;

use crate::completion::TokenStream;
use crate::error::ParseError;

/// Parsed command arguments (internal utility, hidden from users)
#[derive(Debug, Clone)]
pub struct ParsedArgs {
    /// Raw input string
    pub raw: String,

    /// Parsed tokens (uses TokenStream for consistency)
    tokens: TokenStream,
}

impl ParsedArgs {
    /// Create ParsedArgs from raw string
    pub fn new(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        let tokens = TokenStream::from_quoted(&raw);

        Self { raw, tokens }
    }

    /// Get positional arg by index
    pub fn get(&self, index: usize) -> Option<&str> {
        self.tokens.get(index)
    }

    /// Parse arg at position as type T
    pub fn parse<T>(&self, index: usize) -> Result<T, ParseError>
    where
        T: FromStr,
        T::Err: std::fmt::Display + std::fmt::Debug + Send + Sync + 'static,
    {
        self.tokens.parse(index)
            .ok_or(ParseError::MissingArg(index))
            .and_then(|v| Ok(v))
            .or_else(|_| {
                // If parse failed, try to get the value for better error message
                let value = self.get(index).ok_or(ParseError::MissingArg(index))?;
                value.parse().map_err(|e| ParseError::Custom(format!("Failed to parse argument at position {}: {}", index, e)))
            })
    }

    /// Try to parse optional arg at position
    pub fn parse_optional<T>(&self, index: usize) -> Result<Option<T>, ParseError>
    where
        T: FromStr,
        T::Err: std::fmt::Display + std::fmt::Debug + Send + Sync + 'static,
    {
        if self.get(index).is_some() {
            self.parse(index).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Number of parsed tokens
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    /// Check if no args were provided
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_args() {
        let args = ParsedArgs::new("foo bar baz");
        assert_eq!(args.get(0), Some("foo"));
        assert_eq!(args.get(1), Some("bar"));
        assert_eq!(args.get(2), Some("baz"));
    }

    #[test]
    fn test_parse_quoted_args() {
        let args = ParsedArgs::new(r#"foo "hello world" bar"#);
        assert_eq!(args.get(0), Some("foo"));
        assert_eq!(args.get(1), Some("hello world"));
        assert_eq!(args.get(2), Some("bar"));
    }

    #[test]
    fn test_parse_escaped_quotes() {
        let args = ParsedArgs::new(r#""hello \"world\"""#);
        assert_eq!(args.get(0), Some(r#"hello "world""#));
    }

    #[test]
    fn test_parse_type() {
        let args = ParsedArgs::new("42 3.14");
        assert_eq!(args.parse::<i32>(0).unwrap(), 42);
        assert_eq!(args.parse::<f64>(1).unwrap(), 3.14);
    }

    #[test]
    fn test_parse_missing_arg() {
        let args = ParsedArgs::new("foo");
        assert!(matches!(args.parse::<String>(1), Err(ParseError::MissingArg(1))));
    }

    #[test]
    fn test_parse_optional() {
        let args = ParsedArgs::new("foo");
        assert_eq!(args.parse_optional::<String>(0).unwrap(), Some("foo".to_string()));
        assert_eq!(args.parse_optional::<String>(1).unwrap(), None);
    }
}
