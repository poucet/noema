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
