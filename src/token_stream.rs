/// Parsed tokens with methods for consumption
#[derive(Clone, Debug)]
pub struct TokenStream {
    /// Original input string
    input: String,

    /// Cursor position in input
    cursor: usize,

    /// Parsed tokens
    tokens: Vec<String>,
}

impl TokenStream {
    /// Create TokenStream with simple whitespace tokenization (for completion)
    pub fn new(input: String, cursor: usize) -> Self {
        let tokens = input.split_whitespace().map(String::from).collect();
        Self { input, cursor, tokens }
    }

    /// Create TokenStream respecting quotes (for command argument parsing)
    pub fn from_quoted(input: impl Into<String>) -> Self {
        let input = input.into();
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

        Self { input, cursor: 0, tokens }
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
            self.last().unwrap_or("")
        }
    }

    /// Get the original input string
    pub fn input(&self) -> &str {
        &self.input
    }
}
