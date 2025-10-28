use std::str::FromStr;

use crate::error::ParseError;

/// Parsed command arguments (internal utility, hidden from users)
#[derive(Debug, Clone)]
pub struct ParsedArgs {
    /// Raw input string
    pub raw: String,

    /// Parsed tokens (respects quotes)
    pub tokens: Vec<String>,
}

impl ParsedArgs {
    /// Create ParsedArgs from raw string
    pub fn new(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        let tokens = parse_tokens(&raw);

        Self { raw, tokens }
    }

    /// Get positional arg by index
    pub fn get(&self, index: usize) -> Option<&str> {
        self.tokens.get(index).map(|s| s.as_str())
    }

    /// Get all args after index (for variadic args in future)
    pub fn rest(&self, from: usize) -> &[String] {
        if from < self.tokens.len() {
            &self.tokens[from..]
        } else {
            &[]
        }
    }

    /// Parse arg at position as type T
    pub fn parse<T>(&self, index: usize) -> Result<T, ParseError>
    where
        T: FromStr,
        T::Err: std::error::Error + Send + Sync + 'static,
    {
        let value = self
            .get(index)
            .ok_or(ParseError::MissingArg(index))?;

        value.parse().map_err(|e| ParseError::InvalidType {
            position: index,
            expected: std::any::type_name::<T>().to_string(),
            source: Box::new(e),
        })
    }

    /// Try to parse optional arg at position
    pub fn parse_optional<T>(&self, index: usize) -> Result<Option<T>, ParseError>
    where
        T: FromStr,
        T::Err: std::error::Error + Send + Sync + 'static,
    {
        match self.get(index) {
            Some(_) => self.parse(index).map(Some),
            None => Ok(None),
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

/// Parse tokens from input string, respecting quotes
fn parse_tokens(input: &str) -> Vec<String> {
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

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_args() {
        let args = ParsedArgs::new("foo bar baz");
        assert_eq!(args.tokens, vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn test_parse_quoted_args() {
        let args = ParsedArgs::new(r#"foo "hello world" bar"#);
        assert_eq!(args.tokens, vec!["foo", "hello world", "bar"]);
    }

    #[test]
    fn test_parse_escaped_quotes() {
        let args = ParsedArgs::new(r#""hello \"world\"""#);
        assert_eq!(args.tokens, vec![r#"hello "world""#]);
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
