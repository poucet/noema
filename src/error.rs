use std::fmt;

/// Errors that can occur during completion
#[derive(Debug, Clone)]
pub enum CompletionError {
    /// API or service unavailable
    ServiceUnavailable(String),

    /// Invalid context for completion
    InvalidContext(String),

    /// Custom error
    Custom(String),
}

impl fmt::Display for CompletionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompletionError::ServiceUnavailable(msg) => write!(f, "Service unavailable: {}", msg),
            CompletionError::InvalidContext(msg) => write!(f, "Invalid context: {}", msg),
            CompletionError::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for CompletionError {}

/// Errors that can occur during command execution
#[derive(Debug)]
pub enum CommandError {
    /// Command not found
    UnknownCommand(String),

    /// Failed to parse arguments
    ParseError(ParseError),

    /// Error during command execution
    ExecutionError(String),

    /// Invalid arguments provided
    InvalidArgs(String),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::UnknownCommand(cmd) => write!(f, "Unknown command: {}", cmd),
            CommandError::ParseError(err) => write!(f, "Parse error: {}", err),
            CommandError::ExecutionError(msg) => write!(f, "Execution error: {}", msg),
            CommandError::InvalidArgs(msg) => write!(f, "Invalid arguments: {}", msg),
        }
    }
}

impl std::error::Error for CommandError {}

impl From<ParseError> for CommandError {
    fn from(err: ParseError) -> Self {
        CommandError::ParseError(err)
    }
}

/// Errors that can occur during argument parsing
#[derive(Debug)]
pub enum ParseError {
    /// Missing required argument at position
    MissingArg(usize),

    /// Invalid type conversion for argument
    InvalidType {
        position: usize,
        expected: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Generic parse error
    Custom(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::MissingArg(pos) => write!(f, "Missing argument at position {}", pos),
            ParseError::InvalidType { position, expected, source } => {
                write!(f, "Invalid type for argument at position {}: expected {}, error: {}", position, expected, source)
            }
            ParseError::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for ParseError {}
